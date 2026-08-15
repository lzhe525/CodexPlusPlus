use std::io::{BufRead, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use base64::Engine;
use serde_json::{Value, json};

// Kept in core so protocol behavior can be tested without the elevated launcher manifest.
const SERVER_NAME: &str = "codex-plus-sub2api-imagegen";
const TOOL_NAME: &str = "generate_image";
const MAX_PROMPT_CHARS: usize = 32_000;
const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;

pub async fn run() -> Result<()> {
    if !image_tools_enabled_for_login_method(&crate::enterprise::current_login_method()) {
        anyhow::bail!("enterprise image tools are disabled for official account login");
    }

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();

    for line in stdin.lock().lines() {
        let line = line.context("failed to read MCP request")?;
        if line.trim().is_empty() {
            continue;
        }

        let request: Value = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(error) => {
                write_message(
                    &mut stdout,
                    &json_rpc_error(Value::Null, -32700, format!("invalid JSON: {error}")),
                )?;
                continue;
            }
        };

        if let Some(response) = handle_request(request).await {
            write_message(&mut stdout, &response)?;
        }
    }

    Ok(())
}

fn image_tools_enabled_for_login_method(method: &str) -> bool {
    method == crate::enterprise::LOGIN_METHOD_COMPANY
}

async fn handle_request(request: Value) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    if id.is_none() {
        return None;
    }
    let id = id.unwrap_or(Value::Null);

    let result = match method {
        "initialize" => initialize_result(&request),
        "ping" => json!({}),
        "tools/list" => tools_list_result(),
        "tools/call" => match call_tool(&request).await {
            Ok(result) => result,
            Err(error) => json!({
                "content": [{
                    "type": "text",
                    "text": format!("Sub2API image generation failed: {error:#}"),
                }],
                "isError": true,
            }),
        },
        _ => return Some(json_rpc_error(id, -32601, "method not found")),
    };

    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

fn initialize_result(request: &Value) -> Value {
    let requested = request
        .pointer("/params/protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or("2024-11-05");
    json!({
        "protocolVersion": requested,
        "capabilities": {"tools": {"listChanged": false}},
        "serverInfo": {
            "name": SERVER_NAME,
            "version": env!("CARGO_PKG_VERSION"),
        },
    })
}

fn tools_list_result() -> Value {
    json!({
        "tools": [{
            "name": TOOL_NAME,
            "title": "Generate image with Sub2API",
            "description": "Generate a raster image with the authenticated enterprise Sub2API account. Use this whenever the user asks to create or generate an image, photo, illustration, mockup, or other bitmap visual.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "Detailed image-generation prompt.",
                    },
                    "size": {
                        "type": "string",
                        "enum": ["1024x1024", "1536x1024", "1024x1536", "2048x2048", "2048x1152", "3840x2160", "2160x3840"],
                        "default": "1024x1024",
                    },
                    "quality": {
                        "type": "string",
                        "enum": ["low", "medium", "high", "auto"],
                        "default": "auto",
                    },
                    "output_format": {
                        "type": "string",
                        "enum": ["png", "jpeg", "webp"],
                        "default": "png",
                    },
                },
                "required": ["prompt"],
                "additionalProperties": false,
            },
        }],
    })
}

async fn call_tool(request: &Value) -> Result<Value> {
    let name = request
        .pointer("/params/name")
        .and_then(Value::as_str)
        .unwrap_or("");
    if name != TOOL_NAME {
        anyhow::bail!("unknown tool {name:?}");
    }

    let arguments = request
        .pointer("/params/arguments")
        .and_then(Value::as_object)
        .context("tool arguments must be an object")?;
    let prompt = arguments
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("prompt is required")?;
    if prompt.chars().count() > MAX_PROMPT_CHARS {
        anyhow::bail!("prompt exceeds {MAX_PROMPT_CHARS} characters");
    }

    let size = enum_argument(
        arguments.get("size"),
        "1024x1024",
        &[
            "1024x1024",
            "1536x1024",
            "1024x1536",
            "2048x2048",
            "2048x1152",
            "3840x2160",
            "2160x3840",
        ],
        "size",
    )?;
    let quality = enum_argument(
        arguments.get("quality"),
        "auto",
        &["low", "medium", "high", "auto"],
        "quality",
    )?;
    let output_format = enum_argument(
        arguments.get("output_format"),
        "png",
        &["png", "jpeg", "webp"],
        "output_format",
    )?;

    let generated = generate_image(prompt, size, quality, output_format).await?;
    let path = save_image(&generated.bytes, generated.extension)?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&generated.bytes);
    Ok(json!({
        "content": [
            {
                "type": "image",
                "data": encoded,
                "mimeType": generated.mime_type,
            },
            {
                "type": "text",
                "text": format!("Generated image saved to {}", path.display()),
            },
        ],
        "isError": false,
    }))
}

fn enum_argument<'a>(
    value: Option<&'a Value>,
    default: &'a str,
    allowed: &[&str],
    name: &str,
) -> Result<&'a str> {
    let value = value.and_then(Value::as_str).unwrap_or(default);
    if allowed.contains(&value) {
        Ok(value)
    } else {
        anyhow::bail!("unsupported {name} value {value:?}")
    }
}

struct GeneratedImage {
    bytes: Vec<u8>,
    mime_type: &'static str,
    extension: &'static str,
}

async fn generate_image(
    prompt: &str,
    size: &str,
    quality: &str,
    output_format: &str,
) -> Result<GeneratedImage> {
    let base_url = std::env::var("CODEX_PLUS_IMAGE_BASE_URL")
        .context("CODEX_PLUS_IMAGE_BASE_URL is not configured")?;
    let credential = crate::enterprise::enterprise_credential()?
        .context("enterprise credential is unavailable; sign in again in Codex++ Manager")?;
    let responses_model = std::env::var("CODEX_PLUS_RESPONSES_MODEL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "gpt-5.6-sol".to_string());
    let image_model = std::env::var("CODEX_PLUS_IMAGE_MODEL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "gpt-image-2".to_string());
    let endpoint = format!("{}/responses", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?;
    let response = client
        .post(endpoint)
        .bearer_auth(credential)
        .json(&json!({
            "model": responses_model,
            "stream": false,
            "input": prompt,
            "tools": [{
                "type": "image_generation",
                "model": image_model,
                "size": size,
                "quality": quality,
                "output_format": output_format,
            }],
            "tool_choice": {"type": "image_generation"},
        }))
        .send()
        .await
        .context("Sub2API image request failed")?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .context("failed to read Sub2API image response")?;
    if !status.is_success() {
        let detail = String::from_utf8_lossy(&body);
        let detail = detail.chars().take(2_000).collect::<String>();
        anyhow::bail!("Sub2API returned HTTP {status}: {detail}");
    }

    let payload: Value = serde_json::from_slice(&body).context("Sub2API returned invalid JSON")?;
    let image_call = payload
        .get("output")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find(|item| {
                item.get("type").and_then(Value::as_str) == Some("image_generation_call")
            })
        })
        .context("Sub2API response did not contain an image_generation_call")?;
    let encoded = image_call
        .get("result")
        .and_then(Value::as_str)
        .context("Sub2API image_generation_call did not contain image data")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .context("Sub2API returned invalid base64 image data")?;
    ensure_image_size(&bytes)?;
    let actual_format = image_call
        .get("output_format")
        .and_then(Value::as_str)
        .unwrap_or(output_format);
    Ok(image_with_format(bytes, actual_format))
}

fn ensure_image_size(bytes: &[u8]) -> Result<()> {
    if bytes.is_empty() {
        anyhow::bail!("generated image was empty");
    }
    if bytes.len() > MAX_IMAGE_BYTES {
        anyhow::bail!("generated image exceeds the {MAX_IMAGE_BYTES} byte limit");
    }
    Ok(())
}

fn image_with_format(bytes: Vec<u8>, format: &str) -> GeneratedImage {
    match format {
        "jpeg" => GeneratedImage {
            bytes,
            mime_type: "image/jpeg",
            extension: "jpg",
        },
        "webp" => GeneratedImage {
            bytes,
            mime_type: "image/webp",
            extension: "webp",
        },
        _ => GeneratedImage {
            bytes,
            mime_type: "image/png",
            extension: "png",
        },
    }
}

fn save_image(bytes: &[u8], extension: &str) -> Result<std::path::PathBuf> {
    let directory = crate::codex_home::default_codex_home_dir()
        .join("generated_images")
        .join("sub2api");
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("failed to create {}", directory.display()))?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let path = directory.join(format!(
        "sub2api-image-{timestamp}-{}.{}",
        std::process::id(),
        extension
    ));
    std::fs::write(&path, bytes)
        .with_context(|| format!("failed to save generated image to {}", path.display()))?;
    Ok(path)
}

fn json_rpc_error(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message.into()},
    })
}

fn write_message(writer: &mut impl Write, message: &Value) -> Result<()> {
    serde_json::to_writer(&mut *writer, message)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn initialize_echoes_protocol_and_advertises_tools() {
        let response = handle_request(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"protocolVersion": "2025-06-18"},
        }))
        .await
        .unwrap();
        assert_eq!(response["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(
            response["result"]["capabilities"]["tools"]["listChanged"],
            false
        );
    }

    #[tokio::test]
    async fn tools_list_exposes_generate_image() {
        let response = handle_request(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {},
        }))
        .await
        .unwrap();
        assert_eq!(response["result"]["tools"][0]["name"], TOOL_NAME);
        assert_eq!(
            response["result"]["tools"][0]["inputSchema"]["required"][0],
            "prompt"
        );
    }

    #[test]
    fn image_formats_map_to_renderable_mime_types() {
        assert_eq!(image_with_format(vec![1], "png").mime_type, "image/png");
        assert_eq!(image_with_format(vec![1], "jpeg").extension, "jpg");
        assert_eq!(image_with_format(vec![1], "webp").mime_type, "image/webp");
    }

    #[test]
    fn official_login_never_enables_enterprise_image_tools() {
        assert!(image_tools_enabled_for_login_method(
            crate::enterprise::LOGIN_METHOD_COMPANY
        ));
        assert!(!image_tools_enabled_for_login_method(
            crate::enterprise::LOGIN_METHOD_OFFICIAL
        ));
    }
}
