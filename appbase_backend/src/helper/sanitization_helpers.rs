//  this below code do markdwon to html and sanitaize to.

// // File: src/helper/sanitization_helpers.rs
// use ammonia::Builder;
// use pulldown_cmark::{html, Options, Parser};
// use std::collections::HashSet;

// /// Sanitizes a Markdown string by converting it to HTML and then cleaning it.
// /// This allows a safe subset of HTML tags and inline styles for rich post content.
// /// It strictly removes all scripting capabilities (`onclick`, `onerror`, etc.).
// // Replace the existing function
// pub fn sanitize_markdown_content(markdown_input: &str) -> String {
//     // 1. Configure pulldown-cmark parser with common extensions.
//     let mut options = Options::empty();
//     options.insert(Options::ENABLE_TABLES);
//     options.insert(Options::ENABLE_FOOTNOTES);
//     options.insert(Options::ENABLE_STRIKETHROUGH);
//     options.insert(Options::ENABLE_TASKLISTS);

//     // 2. Parse the Markdown into an HTML string.
//     let parser = Parser::new_ext(markdown_input, options);
//     let mut unsafe_html = String::new();
//     html::push_html(&mut unsafe_html, parser);

//     // 3. Define a whitelist of safe tags.
//     let tags_to_allow = [
//         "h1", "h2", "h3", "h4", "h5", "h6", "b", "strong", "i", "em", "p", "br",
//         "a", "ul", "ol", "li", "blockquote", "code", "pre", "hr", "img", "table",
//         "thead", "tbody", "tr", "th", "td", "s", "del", "video", "div"
//     ];
//     let safe_tags = tags_to_allow.iter().cloned().collect::<HashSet<_>>();

//     // 4. Define a whitelist of safe attributes.
//     let safe_attributes = [
//         "src", "href", "alt", "title", "class", "style", "controls", "width", "height", "align"
//     ];
//     let generic_attributes = safe_attributes.iter().cloned().collect::<HashSet<_>>();

//     // 5. Sanitize the HTML with Ammonia.
//     let clean_html = Builder::new()
//         .tags(safe_tags)
//         .generic_attributes(generic_attributes)
//         .link_rel(Some("nofollow ugc")) // Security best practice for user links
//         .clean(&unsafe_html)
//         .to_string();

//     clean_html
// }

// /// Strips all HTML tags from a string, leaving only the plain text content.
// /// This is the correct method for fields like titles and summaries, as it
// /// removes unwanted tags entirely rather than displaying ugly escaped characters.
// pub fn strip_all_html(input: &str) -> String {
//     ammonia::Builder::new()
//         .tags(HashSet::new()) // Allow no tags
//         .clean(input)
//         .to_string()
// }


// in our use we focus on markdown only so we go only markdwon and save markwon to db . all other html tag auto escape . 


// this below code mainly do auto escape

// use std::collections::{HashMap, HashSet};
// use regex::Regex;

// /// Sanitizes Markdown content by stripping all HTML tags from the input,
// /// while intelligently preserving the content of fenced code blocks (```) untouched.
// pub fn sanitize_markdown_content(markdown_input: &str) -> String {
//     // A vector to temporarily store the original code blocks.
//     let mut code_blocks: Vec<String> = Vec::new();

//     // This regex finds fenced code blocks. The `(?s)` flag allows `.` to match newlines.
//     let code_block_regex = Regex::new(r"(?s)```[\s\S]*?```").unwrap();

//     // Step 1: Find all code blocks and replace them with a unique placeholder.
//     let with_placeholders = code_block_regex.replace_all(markdown_input, |caps: &regex::Captures| {
//         // Store the original code block (the full match)
//         code_blocks.push(caps[0].to_string());
//         // Return a placeholder with a unique index.
//         format!("__CODE_BLOCK_PLACEHOLDER_{}__", code_blocks.len() - 1)
//     });

//     // Step 2: Sanitize the string, which now only has placeholders where the code blocks were.
//     // Ammonia will not touch the placeholders.
//     let sanitized_with_placeholders = ammonia::Builder::new()
//         .tags(HashSet::new())
//         .tag_attributes(HashMap::new())
//         .generic_attributes(HashSet::new())
//         .link_rel(None)
//         .strip_comments(true)
//         .clean(&with_placeholders)
//         .to_string();

//     // Step 3: Restore the original code blocks.
//     let mut final_output = sanitized_with_placeholders;
//     for (i, block) in code_blocks.iter().enumerate() {
//         let placeholder = format!("__CODE_BLOCK_PLACEHOLDER_{}__", i);
//         // We use `replacen` to only replace the first occurrence, just in case.
//         final_output = final_output.replacen(&placeholder, block, 1);
//     }

//     final_output
// }

// /// This function remains unchanged and is still correct for titles/summaries.
// pub fn strip_all_html(input: &str) -> String {
//     ammonia::Builder::new()
//         .tags(HashSet::new())
//         .clean(input)
//         .to_string()
// }



use regex::Regex;

/// Sanitizes Markdown content by escaping all HTML tags outside code blocks,
/// while preserving fenced code blocks (```) untouched.
/// Prevents double-escaping by normalizing entities first.
pub fn sanitize_markdown_content(markdown_input: &str) -> String {
    let mut code_blocks: Vec<String> = Vec::new();
    let code_block_regex = Regex::new(r"(?s)```[\s\S]*?```").unwrap();

    // Step 1: Extract code blocks with placeholders
    let with_placeholders = code_block_regex.replace_all(markdown_input, |caps: &regex::Captures| {
        code_blocks.push(caps[0].to_string());
        format!("__CODE_BLOCK_PLACEHOLDER_{}__", code_blocks.len() - 1)
    });

    // Step 2: Decode existing entities (normalize), then escape HTML
    let decoded = html_escape::decode_html_entities(&with_placeholders);
    let escaped = html_escape::encode_text(&decoded).to_string();

    // Step 3: Restore original code blocks
    let mut final_output = escaped;
    for (i, block) in code_blocks.iter().enumerate() {
        let placeholder = format!("__CODE_BLOCK_PLACEHOLDER_{}__", i);
        final_output = final_output.replacen(&placeholder, block, 1);
    }

    final_output
}

/// Strips all HTML tags from input (for titles/summaries)
pub fn strip_all_html(input: &str) -> String {
    use std::collections::HashSet;
    ammonia::Builder::new()
        .tags(HashSet::new())
        .clean(input)
        .to_string()
}