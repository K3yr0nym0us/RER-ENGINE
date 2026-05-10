use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct SharedBinaryStatus {
    status: &'static str,
    message: &'static str,
}

fn main() {
    let payload = SharedBinaryStatus {
        status: "ok",
        message: "rer_engine_shared listo",
    };

    if let Ok(json) = serde_json::to_string(&payload) {
        println!("{json}");
    }
}
