#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Install panic hook for structured production error logging
    install_panic_hook();

    // Load .env early; ignore if missing.
    dotenvy::dotenv().ok();

    match coding_agent_search::run().await {
        Ok(()) => Ok(()),
        Err(err) => {
            // If the message looks like JSON, output it directly (it's a pre-formatted robot error)
            if err.message.trim().starts_with('{') {
                eprintln!("{}", err.message);
            } else {
                // Otherwise wrap structured error
                let payload = serde_json::json!({
                    "error": {
                        "code": err.code,
                        "kind": err.kind,
                        "message": err.message,
                        "hint": err.hint,
                        "retryable": err.retryable,
                    }
                });
                eprintln!("{payload}");
            }
            std::process::exit(err.code);
        }
    }
}

/// Install a panic hook that outputs structured JSON to stderr for production observability.
///
/// This ensures panics are captured with:
/// - Human-readable message
/// - Source location (file:line:column)
/// - Full backtrace for debugging
/// - Thread name for context
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        let backtrace = std::backtrace::Backtrace::capture();

        let location = panic_info.location().map(|l| {
            serde_json::json!({
                "file": l.file(),
                "line": l.line(),
                "column": l.column(),
            })
        });

        let payload = serde_json::json!({
            "panic": {
                "message": panic_info.to_string(),
                "location": location,
                "backtrace": format!("{backtrace}"),
                "thread": std::thread::current().name().unwrap_or("unknown"),
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }
        });

        // Output to stderr for log aggregators
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| panic_info.to_string())
        );
    }));
}
