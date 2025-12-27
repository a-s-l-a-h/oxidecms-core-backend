use actix_web::{
    // We bring `EitherBody` into scope to help the compiler, though we use it via a helper method.
    body::EitherBody,
    dev::{self, forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    guard, web, Error, FromRequest, HttpRequest, HttpResponse,
};
use actix_session::{Session, SessionExt};
use futures_util::future::{ok, LocalBoxFuture, Ready};
use serde::Serialize;
use std::env;
use std::future::{ready, Ready as StdReady};
use crate::AppState;

#[derive(Serialize)]
pub struct AuthenticatedContributor {
    pub username: String,
    pub role: String,
}

impl FromRequest for AuthenticatedContributor {
    type Error = actix_web::Error;
    type Future = StdReady<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _: &mut dev::Payload) -> Self::Future {
        let session = req.get_session();
        if let (Ok(Some(username)), Ok(Some(role))) = (session.get("username"), session.get("role")) {
            ready(Ok(AuthenticatedContributor { username, role }))
        } else {
            ready(Err(actix_web::error::ErrorUnauthorized("Not logged in.")))
        }
    }
}

pub fn admin_guard(session: &Session) -> bool {
    session.get::<String>("role").unwrap_or(None) == Some("admin".to_string())
}

pub fn contributor_guard(session: &Session) -> bool {
    // --- MODIFIED LINE ---
    // Now this guard ONLY allows the 'contributor' role, completely separating it from 'admin'.
    session.get::<String>("role").unwrap_or(None) == Some("contributor".to_string())
}

pub fn ip_guard(ctx: &guard::GuardContext) -> bool {
    let allowed_ips_str = match env::var("ADMIN_LOGIN_ACCEPT_IP") {
        Ok(val) => val,
        Err(_) => {
            log::warn!("ADMIN_LOGIN_ACCEPT_IP is not set. Denying all admin login attempts.");
            return false;
        }
    };

    if allowed_ips_str.trim() == "*" {
        return true;
    }

    // UPDATED: Get the real IP, considering reverse proxies
    let request_ip = ctx.head().headers()
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next()) // Take the first IP if there's a list
        .map(|s| s.trim().to_string())
        .or_else(|| {
            ctx.head().peer_addr.map(|addr| addr.ip().to_string())
        });

    let peer_addr = match request_ip {
        Some(ip) => ip,
        None => {
            log::warn!("Could not determine peer IP address for admin login attempt.");
            return false;
        }
    };

    let is_allowed = allowed_ips_str.split(',').any(|ip| ip.trim() == peer_addr);

    if !is_allowed {
        log::warn!("Blocked admin login attempt from unauthorized IP: {}", peer_addr);
    }

    is_allowed
}


// --- The correct implementation using Middleware ---

pub struct ContributorPrefixValidation;

impl<S, B> Transform<S, ServiceRequest> for ContributorPrefixValidation
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>; // The response can be one of two body types
    type Error = Error;
    type InitError = ();
    type Transform = ContributorPrefixValidationMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(ContributorPrefixValidationMiddleware { service })
    }
}

pub struct ContributorPrefixValidationMiddleware<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for ContributorPrefixValidationMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>; // The response can be one of two body types
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let state_opt = req.app_data::<web::Data<AppState>>();
        let prefix_from_url_opt = req.match_info().get("prefix").map(|s| s.to_string());

        let is_valid = match (state_opt, prefix_from_url_opt) {
            (Some(app_state), Some(prefix_from_url)) => {
                // UPDATED: Handle poisoned RwLock gracefully
                let current_secret_prefix = app_state.contributor_prefix.read()
                    .unwrap_or_else(|poisoned| {
                        log::error!("RwLock for contributor_prefix was poisoned! Using stale data.");
                        poisoned.into_inner() // Recover the lock
                    });
                prefix_from_url == *current_secret_prefix
            }
            _ => false,
        };

        if is_valid {
            let fut = self.service.call(req);
            Box::pin(async move {
                // The service returns a ServiceResponse<B>, we map it into the "left" side
                let res = fut.await?;
                Ok(res.map_into_left_body())
            })
        } else {
            Box::pin(async move {
                // We create a new response and map it into the "right" side
                let (http_req, _payload) = req.into_parts();
                let res = HttpResponse::NotFound().finish().map_into_right_body();
                Ok(ServiceResponse::new(http_req, res))
            })
        }
    }
}