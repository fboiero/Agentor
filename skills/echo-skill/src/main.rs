use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Input {
    message: String,
}

#[derive(Serialize)]
struct Output {
    echo: String,
}

fn main() {
    // Read input from command line args (passed by WASI)
    let args: Vec<String> = std::env::args().collect();
    let input_str = args.get(1).map(|s| s.as_str()).unwrap_or("{}");

    let response = match serde_json::from_str::<Input>(input_str) {
        Ok(input) => {
            let output = Output {
                echo: format!("Echo: {}", input.message),
            };
            serde_json::to_string(&output).unwrap_or_default()
        }
        Err(_) => {
            // If no structured input, just echo whatever we got
            let output = Output {
                echo: format!("Echo: {}", input_str),
            };
            serde_json::to_string(&output).unwrap_or_default()
        }
    };

    println!("{}", response);
}
