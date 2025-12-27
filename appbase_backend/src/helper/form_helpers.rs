use actix_web::{web, HttpResponse};
use std::collections::HashMap;
use url::form_urlencoded;

/// Parses URL-encoded form data from bytes, handling potential UTF-8 errors gracefully.
pub fn parse_form(form_bytes: &web::Bytes) -> Result<HashMap<String, String>, HttpResponse> {
    let body = match String::from_utf8(form_bytes.to_vec()) {
        Ok(s) => s,
        Err(_) => return Err(HttpResponse::BadRequest().body("Invalid UTF-8 in request body.")),
    };
    Ok(form_urlencoded::parse(body.as_bytes()).into_owned().collect())
}