use std::env;
use std::io::{self, Write};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, AUTHORIZATION};
use serde::{Deserialize, Serialize};

/// Request structure for chat completions.
#[derive(Serialize)]
struct OpenRouterChatRequest {
    model: String,
    messages: Vec<ChatMessageRequest>,
}

/// A chat message for the request.
#[derive(Serialize, Clone)] // <-- Derive Clone here.
struct ChatMessageRequest {
    role: String,
    content: String,
}

/// Response structure for chat completions.
#[derive(Deserialize, Debug)]
struct OpenRouterChatResponse {
    id: String,
    object: String,
    created: u64,
    choices: Vec<ChatChoice>,
}

/// A single choice from the response.
#[derive(Deserialize, Debug)]
struct ChatChoice {
    #[serde(default)]
    index: Option<u32>,
    message: ChatMessage,
    finish_reason: Option<String>,
}

/// A chat message in the response.
#[derive(Deserialize, Debug)]
struct ChatMessage {
    role: String,
    content: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables from .env if available.
    dotenv::dotenv().ok();

    // Retrieve the API key from the environment.
    let api_key = env::var("OPENROUTER_API_KEY")
        .expect("OPENROUTER_API_KEY must be set in the environment");

    // Use the chat completions endpoint by default.
    let url = env::var("OPENROUTER_API_URL")
        .unwrap_or_else(|_| "https://openrouter.ai/api/v1/chat/completions".to_string());

    // Optional headers for HTTP-Referer and X-Title.
    let http_referer = env::var("HTTP_REFERER").ok();
    let x_title = env::var("X_TITLE").ok();

    // Prepare the reqwest client and base headers.
    let client = reqwest::Client::new();
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", api_key))?,
    );
    if let Some(referer) = http_referer {
        headers.insert("HTTP-Referer", HeaderValue::from_str(&referer)?);
    }
    if let Some(title) = x_title {
        headers.insert("X-Title", HeaderValue::from_str(&title)?);
    }

    println!("Chat with the LLM. Type your message and press Enter. Type 'quit' to exit.");

    // Maintain a conversation history.
    let mut conversation: Vec<ChatMessageRequest> = Vec::new();
    let stdin = io::stdin();

    loop {
        print!("> ");
        io::stdout().flush()?;

        let mut user_input = String::new();
        stdin.read_line(&mut user_input)?;
        let user_input = user_input.trim();

        if user_input.eq_ignore_ascii_case("quit") {
            break;
        }

        if user_input.is_empty() {
            continue;
        }

        // Add the user's message to the conversation.
        conversation.push(ChatMessageRequest {
            role: "user".to_string(),
            content: user_input.to_string(),
        });

        // Build the request payload.
        let request_body = OpenRouterChatRequest {
            model: "cognitivecomputations/dolphin3.0-mistral-24b:free".to_string(),
            messages: conversation.clone(),
        };

        // Send the POST request.
        let resp = client
            .post(&url)
            .headers(headers.clone())
            .json(&request_body)
            .send()
            .await?;

        if !resp.status().is_success() {
            println!("Request failed with status: {}", resp.status());
            let error_text = resp.text().await?;
            println!("Error details: {}", error_text);
            continue;
        }

        // Read and deserialize the response.
        let response_text = resp.text().await?;
        let chat_response: OpenRouterChatResponse = match serde_json::from_str(&response_text) {
            Ok(resp) => resp,
            Err(e) => {
                println!("Failed to parse response: {}", e);
                println!("Raw response: {}", response_text);
                continue;
            }
        };

        // Extract and print the assistant's message.
        if let Some(choice) = chat_response.choices.first() {
            println!("LLM: {}", choice.message.content);
            // Append the assistant's message to the conversation.
            conversation.push(ChatMessageRequest {
                role: "assistant".to_string(),
                content: choice.message.content.clone(),
            });
        } else {
            println!("No message received from LLM.");
        }
    }

    Ok(())
}
