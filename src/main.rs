use std::env;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use eframe::{egui, App};
use egui::{Align, Color32, FontId, Layout, RichText, Rounding, Stroke, TextStyle, Vec2};
// Add this import for Margin
use egui::style::Margin;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

/// A chat message that we store in the conversation.
#[derive(Serialize, Clone)]
struct ChatMessageRequest {
    role: String,
    content: String,
    // Add timestamp for showing when messages were sent
    #[serde(skip)]
    timestamp: Instant,
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
    /// Is the assistant currently typing
    is_typing: bool,
    /// The time when typing started (for animation)
    typing_start: Option<Instant>,
    /// Current model being used
    current_model: String,
    /// Dark mode toggle
    dark_mode: bool,
}

impl ChatApp {
    /// Initialize the ChatApp (load environment, prepare headers, etc.).
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Set up custom fonts and styles
        let mut fonts = egui::FontDefinitions::default();
        
        // Configure text styles
        let mut style = (*cc.egui_ctx.style()).clone();
        style.text_styles = [
            (TextStyle::Heading, FontId::new(24.0, egui::FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(16.0, egui::FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(14.0, egui::FontFamily::Monospace)),
            (TextStyle::Button, FontId::new(16.0, egui::FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(10.0, egui::FontFamily::Proportional)),
        ]
        .into();
        cc.egui_ctx.set_style(style);
        
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

        // Create a channel for background => UI thread communication.
        let (tx, rx) = channel();

        // Add a welcome message to start conversation
        let mut conversation = Vec::new();
        conversation.push(ChatMessageRequest {
            role: "assistant".to_string(),
            content: "Hello! I'm an AI assistant. How can I help you today?".to_string(),
            timestamp: Instant::now(),
        });

        Self {
            conversation,
            input: String::new(),
            tx,
            rx,
            api_key,
            url,
            headers,
            is_typing: false,
            typing_start: None,
            current_model: "deepseek/deepseek-chat:free".to_string(),
            dark_mode: false,
        }
    }

    /// Spawns a background thread that sends the request to the model
    /// and then sends only the assistant's content back via the channel.
    fn send_request(
        conversation: Vec<ChatMessageRequest>,
        _api_key: String,
        url: String,
        headers: HeaderMap,
        model: String,
        tx: Sender<ChatMessage>,
    ) {
        thread::spawn(move || {
            // Create a Tokio runtime for asynchronous operations.
            let rt = tokio::runtime::Runtime::new().unwrap();

            // Run async block on that runtime.
            let result = rt.block_on(async move {
                // Small delay to simulate typing time
                tokio::time::sleep(Duration::from_millis(500)).await;
                
                let client = reqwest::Client::new();
                
                // Strip out timestamps before sending
                let api_conversation: Vec<ChatMessageRequest> = conversation
                    .into_iter()
                    .map(|msg| ChatMessageRequest {
                        role: msg.role,
                        content: msg.content,
                        timestamp: msg.timestamp,
                    })
                    .collect();
                
                let request_body = OpenRouterChatRequest {
                    model,
                    messages: api_conversation,
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
    
    // Helper function to format markdown in chat messages
    fn format_message_text(&self, text: &str, ui: &mut egui::Ui) {
        // Basic markdown parsing for code blocks
        let mut in_code_block = false;
        let mut code_block = String::new();
        
        for line in text.lines() {
            if line.trim().starts_with("```") {
                if in_code_block {
                    // End of code block
                    ui.add_space(4.0);
                    let code_frame = egui::Frame::none()
                        .fill(if self.dark_mode { Color32::from_rgb(40, 44, 52) } else { Color32::from_rgb(245, 245, 245) })
                        .rounding(Rounding::same(4.0))
                        .stroke(Stroke::new(1.0, Color32::from_gray(200)));
                    
                    code_frame.show(ui, |ui| {
                        ui.add_space(8.0);
                        ui.style_mut().override_text_style = Some(TextStyle::Monospace);
                        ui.label(code_block.trim());
                        ui.style_mut().override_text_style = None;
                        ui.add_space(8.0);
                    });
                    ui.add_space(4.0);
                    
                    in_code_block = false;
                    code_block.clear();
                } else {
                    // Start of code block
                    in_code_block = true;
                }
            } else if in_code_block {
                code_block.push_str(line);
                code_block.push('\n');
            } else {
                // Regular text, check for basic formatting
                let text = if line.starts_with("# ") {
                    // Heading
                    RichText::new(&line[2..]).size(20.0).strong()
                } else if line.starts_with("## ") {
                    // Subheading
                    RichText::new(&line[3..]).size(18.0).strong()
                } else {
                    // Regular text, check for inline formatting
                    let mut formatted = line.to_string();
                    // Bold
                    if formatted.contains("**") {
                        formatted = formatted.replace("**", "");
                        RichText::new(formatted).strong()
                    } else {
                        RichText::new(formatted)
                    }
                };
                ui.label(text);
            }
        }
        
        // Handle any trailing code block
        if in_code_block && !code_block.is_empty() {
            ui.add_space(4.0);
            let code_frame = egui::Frame::none()
                .fill(if self.dark_mode { Color32::from_rgb(40, 44, 52) } else { Color32::from_rgb(245, 245, 245) })
                .rounding(Rounding::same(4.0))
                .stroke(Stroke::new(1.0, Color32::from_gray(200)));
            
            code_frame.show(ui, |ui| {
                ui.add_space(8.0);
                ui.style_mut().override_text_style = Some(TextStyle::Monospace);
                ui.label(code_block.trim());
                ui.style_mut().override_text_style = None;
                ui.add_space(8.0);
            });
            ui.add_space(4.0);
        }
    }
}

/// The main eframe/egui app implementation.
impl App for ChatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for dark mode
        if self.dark_mode {
            ctx.set_visuals(egui::Visuals::dark());
        } else {
            ctx.set_visuals(egui::Visuals::light());
        }

        // Receive any messages from the background thread.
        if let Ok(msg) = self.rx.try_recv() {
            // Add the new assistant message to the conversation.
            self.conversation.push(ChatMessageRequest {
                role: msg.role,
                content: msg.content,
                timestamp: Instant::now(),
            });
            
            // No longer typing
            self.is_typing = false;
            self.typing_start = None;
        }

        // Top panel with app title and theme toggle
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Claude-like Chat");
                
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(if self.dark_mode { "☀️ Light" } else { "🌙 Dark" }).clicked() {
                        self.dark_mode = !self.dark_mode;
                    }
                    
                    ui.add_space(10.0);
                    ui.label("Model:");
                    
                    // Model selector
                    egui::ComboBox::from_id_source("model_selector")
                        .selected_text(&self.current_model)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.current_model, "deepseek/deepseek-chat:free".to_string(), "DeepSeek Chat");
                            ui.selectable_value(&mut self.current_model, "anthropic/claude-3-5-sonnet".to_string(), "Claude 3.5 Sonnet");
                            ui.selectable_value(&mut self.current_model, "google/gemini-pro".to_string(), "Gemini Pro");
                        });
                });
            });
            ui.separator();
        });

        // Main chat panel
        egui::CentralPanel::default().show(ctx, |ui| {
            // The chat scroll area, leaving space for the input field at bottom
            let available_height = ui.available_height();
            let input_area_height = 100.0;
            
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .max_height(available_height - input_area_height)
                .show(ui, |ui| {
                    ui.add_space(8.0);
                    
                    for msg in &self.conversation {
                        let (bubble_color, text_color, align) = if msg.role == "user" {
                            // User message
                            if self.dark_mode {
                                (Color32::from_rgb(44, 51, 73), Color32::WHITE, Align::RIGHT)
                            } else {
                                (Color32::from_rgb(217, 234, 251), Color32::BLACK, Align::RIGHT)
                            }
                        } else {
                            // Assistant message
                            if self.dark_mode {
                                (Color32::from_rgb(55, 59, 70), Color32::WHITE, Align::LEFT)
                            } else {
                                (Color32::from_rgb(245, 245, 245), Color32::BLACK, Align::LEFT)
                            }
                        };

                        // Set layout based on message sender
                        let layout = if msg.role == "user" {
                            Layout::right_to_left(Align::TOP)
                        } else {
                            Layout::left_to_right(Align::TOP)
                        };
                        
                        ui.with_layout(layout, |ui| {
                            let max_width = ui.available_width() * 0.85; // Max width for bubbles
                            
                            let frame = egui::Frame::none()
                                .fill(bubble_color)
                                .rounding(Rounding::same(12.0))
                                .stroke(Stroke::new(1.0, Color32::from_gray(200)))
                                .inner_margin(Margin::same(12.0))
                                .outer_margin(Margin::same(8.0));

                            frame.show(ui, |ui| {
                                ui.set_max_width(max_width);
                                ui.set_min_width(100.0);
                                
                                // Fix the styled_label method issue
                                ui.label(RichText::new(&msg.role).strong().color(text_color));
                                
                                ui.add_space(4.0);
                                self.format_message_text(&msg.content, ui);
                            });
                        });
                    }
                    
                    // Show typing indicator if assistant is working
                    if self.is_typing {
                        if self.typing_start.is_none() {
                            self.typing_start = Some(Instant::now());
                        }
                        
                        ui.with_layout(Layout::left_to_right(Align::TOP), |ui| {
                            let frame = egui::Frame::none()
                                .fill(if self.dark_mode {
                                    Color32::from_rgb(55, 59, 70)
                                } else {
                                    Color32::from_rgb(245, 245, 245)
                                })
                                .rounding(Rounding::same(12.0))
                                .stroke(Stroke::new(1.0, Color32::from_gray(200)))
                                .inner_margin(Margin::same(12.0))
                                .outer_margin(Margin::same(8.0));

                            frame.show(ui, |ui| {
                                // Animate dots
                                if let Some(start_time) = self.typing_start {
                                    let elapsed = start_time.elapsed().as_millis() as usize / 500;
                                    let dots = match elapsed % 4 {
                                        0 => "",
                                        1 => ".",
                                        2 => "..",
                                        _ => "...",
                                    };
                                    ui.label(format!("Thinking{}", dots));
                                } else {
                                    ui.label("Thinking...");
                                }
                            });
                        });
                    }
                    
                    ui.add_space(8.0);
                });

            // Fixed input area at the bottom with adjustable height
            let frame = egui::Frame::none()
                .fill(if self.dark_mode {
                    Color32::from_rgb(30, 33, 40)
                } else {
                    Color32::from_rgb(250, 250, 250)
                })
                .stroke(Stroke::new(1.0, Color32::from_gray(200)));
                
            frame.show(ui, |ui| {
                ui.add_space(8.0);
                
                // Fix the TextEdit min_size issue
                let text_edit = egui::TextEdit::multiline(&mut self.input)
                    .hint_text("Type your message here...")
                    .desired_width(f32::INFINITY); // Set minimum height while allowing width to be flexible // Use min_size with Vec2 instead of min_height
                
                ui.add(text_edit);
                
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    // Send button
                    let send_button = ui.add_sized(
                        [120.0, 36.0],
                        egui::Button::new(if self.is_typing { "Sending..." } else { "Send" })
                            .fill(if self.dark_mode {
                                Color32::from_rgb(75, 85, 99)
                            } else {
                                Color32::from_rgb(79, 70, 229)
                            })
                    );
                    
                    let should_send = (send_button.clicked() || 
                        (ui.input().key_pressed(egui::Key::Enter) && ui.input().modifiers.ctrl)) &&
                        !self.input.trim().is_empty() && 
                        !self.is_typing;
                        
                    if should_send {
                        let text = self.input.trim().to_string();
                        
                        // Push the user message to conversation
                        self.conversation.push(ChatMessageRequest {
                            role: "user".to_string(),
                            content: text,
                            timestamp: Instant::now(),
                        });

                        // Mark assistant as typing
                        self.is_typing = true;
                        
                        // Clone conversation and send request in background
                        let conv_clone = self.conversation.clone();
                        Self::send_request(
                            conv_clone,
                            self.api_key.clone(),
                            self.url.clone(),
                            self.headers.clone(),
                            self.current_model.clone(),
                            self.tx.clone(),
                        );

                        // Clear the input field
                        self.input.clear();
                    }
                    
                    // Help text
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new("Press Ctrl+Enter to send").size(12.0).color(Color32::from_gray(150)));
                    });
                });
                ui.add_space(8.0);
            });
        });

        // Continuously repaint for typing animation
        if self.is_typing {
            ctx.request_repaint_after(Duration::from_millis(250));
        } else {
            ctx.request_repaint();
        }
    }
}

fn main() {
    let native_options = eframe::NativeOptions {
        initial_window_size: Some(Vec2::new(800.0, 800.0)),
        min_window_size: Some(Vec2::new(400.0, 400.0)),
        ..Default::default()
    };
    
    eframe::run_native(
        "Claude-like Chat",
        native_options,
        Box::new(|cc| Box::new(ChatApp::new(cc))),
    );
}