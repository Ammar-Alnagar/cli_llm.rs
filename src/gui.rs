use std::env;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use eframe::{egui, App};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

/// A chat message that we store in the conversation.
#[derive(Serialize, Clone)]
struct ChatMessageRequest {
    role: String,
    content: String,
}

/// The request body for sending to your model endpoint.
#[derive(Serialize)]
struct OpenRouterChatRequest {
    model: String,
    messages: Vec<ChatMessageRequest>,
}

/// A chat message from the model response.
#[derive(Deserialize, Debug, Clone)]
struct ChatMessage {
    role: String,
    content: String,
}

/// A single choice from the model response.
#[derive(Deserialize, Debug)]
struct ChatChoice {
    #[serde(default)]
    index: Option<u32>,
    message: ChatMessage,
    finish_reason: Option<String>,
}

/// The overall JSON response structure.
#[derive(Deserialize, Debug)]
struct OpenRouterChatResponse {
    id: String,
    object: String,
    created: u64,
    choices: Vec<ChatChoice>,
}

/// The main GUI application state.
struct ChatApp {
    /// Our conversation buffer (both user and assistant messages).
    conversation: Vec<ChatMessageRequest>,
    /// Current input text in the text box.
    input: String,
    /// Sender for background thread => UI thread communication.
    tx: Sender<ChatMessage>,
    /// Receiver for background thread => UI thread communication.
    rx: Receiver<ChatMessage>,
    /// OpenRouter API key (loaded from environment).
    api_key: String,
    /// OpenRouter API endpoint URL.
    url: String,
    /// Pre-built headers (authorization, content-type, etc.).
    headers: HeaderMap,
}

impl ChatApp {
    /// Initialize the ChatApp (load environment, prepare headers, etc.).
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Load environment variables from .env (if present).
        dotenv::dotenv().ok();

        let api_key = env::var("OPENROUTER_API_KEY")
            .expect("OPENROUTER_API_KEY must be set in the environment");
        let url = env::var("OPENROUTER_API_URL")
            .unwrap_or_else(|_| "https://openrouter.ai/api/v1/chat/completions".to_string());

        // Prepare default headers.
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", api_key)).unwrap(),
        );
        if let Ok(referer) = env::var("HTTP_REFERER") {
            headers.insert("HTTP-Referer", HeaderValue::from_str(&referer).unwrap());
        }
        if let Ok(title) = env::var("X_TITLE") {
            headers.insert("X-Title", HeaderValue::from_str(&title).unwrap());
        }

        // Create a channel for background => UI communication.
        let (tx, rx) = channel();

        Self {
            conversation: Vec::new(),
            input: String::new(),
            tx,
            rx,
            api_key,
            url,
            headers,
        }
    }

    /// Spawns a background thread that sends the request to the model
    /// and then sends only the assistant's content back via the channel.
    fn send_request(
        conversation: Vec<ChatMessageRequest>,
        _api_key: String,
        url: String,
        headers: HeaderMap,
        tx: Sender<ChatMessage>,
    ) {
        thread::spawn(move || {
            // Create a Tokio runtime for asynchronous operations.
            let rt = tokio::runtime::Runtime::new().unwrap();

            // Run async block on that runtime.
            let result = rt.block_on(async move {
                let client = reqwest::Client::new();
                let request_body = OpenRouterChatRequest {
                    model: "cognitivecomputations/dolphin3.0-mistral-24b:free".to_string(),
                    messages: conversation,
                };

                // Make the POST request.
                let resp = client
                    .post(&url)
                    .headers(headers)
                    .json(&request_body)
                    .send()
                    .await;

                match resp {
                    Ok(response) => {
                        if !response.status().is_success() {
                            eprintln!("Request failed with status: {}", response.status());
                            return None;
                        }
                        // Read the entire response as text.
                        let response_text = response.text().await.ok()?;
                        // Parse into our typed struct.
                        let chat_response: OpenRouterChatResponse =
                            serde_json::from_str(&response_text).ok()?;

                        // Extract only the first choice's content.
                        if let Some(choice) = chat_response.choices.first() {
                            Some(ChatMessage {
                                role: "assistant".to_string(),
                                content: choice.message.content.clone(),
                            })
                        } else {
                            None
                        }
                    }
                    Err(e) => {
                        eprintln!("Error sending request: {:?}", e);
                        None
                    }
                }
            });

            // If we got a valid message, send it to the main thread.
            if let Some(assistant_msg) = result {
                let _ = tx.send(assistant_msg);
            }
        });
    }
}

/// The main eframe/egui app implementation.
impl App for ChatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Receive any messages from the background thread.
        while let Ok(msg) = self.rx.try_recv() {
            // Add the new assistant message to the conversation.
            self.conversation.push(ChatMessageRequest {
                role: msg.role,
                content: msg.content,
            });
        }

        // Light mode visuals.
        ctx.set_visuals(egui::Visuals::light());

        egui::CentralPanel::default().show(ctx, |ui| {
            // The chat scroll area, leaving space for the input field at bottom.
            egui::ScrollArea::vertical()
                .max_height(ui.available_height() - 50.0)
                .show(ui, |ui| {
                    for msg in &self.conversation {
                        let bubble_color = if msg.role == "user" {
                            egui::Color32::from_rgb(220, 248, 198) // Light green for user
                        } else {
                            egui::Color32::from_rgb(240, 240, 240) // Light gray for assistant
                        };

                        let frame = egui::Frame::none()
                            .fill(bubble_color)
                            .rounding(egui::Rounding::same(8.0))
                            .stroke(egui::Stroke::new(1.0, egui::Color32::LIGHT_GRAY));

                        frame.show(ui, |ui| {
                            ui.add_space(4.0);
                            ui.label(&msg.content);
                            ui.add_space(4.0);
                        });
                        ui.add_space(8.0);
                    }
                });

            // Fixed input bar at the bottom.
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                ui.separator();
                ui.horizontal(|ui| {
                    let input = ui.add(
                        egui::TextEdit::singleline(&mut self.input)
                            .hint_text("Type your message here...")
                            .desired_width(f32::INFINITY),
                    );

                    // Send button or Enter key triggers the request.
                    if ui.button("Send").clicked()
                        || (input.lost_focus() && ui.input().key_pressed(egui::Key::Enter))
                    {
                        let text = self.input.trim();
                        if !text.is_empty() {
                            // Push the user message to conversation.
                            let user_message = ChatMessageRequest {
                                role: "user".to_string(),
                                content: text.to_string(),
                            };
                            self.conversation.push(user_message.clone());

                            // Clone conversation and send request in background.
                            let conv_clone = self.conversation.clone();
                            Self::send_request(
                                conv_clone,
                                self.api_key.clone(),
                                self.url.clone(),
                                self.headers.clone(),
                                self.tx.clone(),
                            );

                            // Clear the input field.
                            self.input.clear();
                        }
                    }
                });
            });
        });

        // Continuously repaint.
        ctx.request_repaint();
    }
}

fn main() {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "ChatGPT GUI - Only Model Response",
        native_options,
        Box::new(|cc| Box::new(ChatApp::new(cc))),
    );
}
