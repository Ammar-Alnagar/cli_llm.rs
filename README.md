
# CLI LLM Chat

A simple command-line interface (CLI) chat application for interacting with a large language model (LLM) using the OpenRouter API. This project is written in Rust and leverages asynchronous HTTP requests to send chat messages and receive responses from the LLM.

## Features

- **Interactive Chat:** Type messages directly in your terminal and receive responses from the LLM.
- **Conversation History:** Maintains conversation context by accumulating messages.
- **Configurable API:** Uses environment variables to set API credentials, endpoint, and optional headers.
- **Built with Rust:** Fast and efficient, built using popular crates like `reqwest`, `tokio`, and `serde`.

## Requirements

- [Rust](https://www.rust-lang.org/tools/install) (stable)
- An API key from [OpenRouter](https://openrouter.ai)
- (Optional) Git for source control

## Getting Started

### 1. Clone the Repository

```bash
git clone https://github.com/Ammar-Alnagar/cli_llm.rs.git
cd cli_llm.rs
```

### 2. Set Up Environment Variables

Create a `.env` file in the project root with the following variables:

```dotenv
OPENROUTER_API_KEY=<your_openrouter_api_key>
OPENROUTER_API_URL=https://openrouter.ai/api/v1/chat/completions
# Optional headers:
HTTP_REFERER=<your_site_url>
X_TITLE=<your_site_title>
```

*Note:* Replace `<your_openrouter_api_key>`, `<your_site_url>`, and `<your_site_title>` with your actual values.

### 3. Build and Run the Application

Use Cargo to build and run the project:

```bash
cargo run --release
```

### 4. Using the CLI Chat

Once running, you can chat with the LLM by typing your message and pressing Enter. Type `quit` to exit the application.

Example session:

```plaintext
Chat with the LLM. Type your message and press Enter. Type 'quit' to exit.
> What is the meaning of life?
LLM: The meaning of life is a deeply personal question...
> quit
```

## Project Structure

- **src/main.rs:**  
  Contains the main CLI application code, including the conversation loop, HTTP client setup, and JSON serialization/deserialization.
  
- **Cargo.toml:**  
  Contains the project dependencies and configuration.

## Contributing

Contributions are welcome! If you'd like to contribute, please fork the repository and create a pull request. Feel free to open issues for bugs or feature requests.

## License

This project is open-source and available under the [MIT License](LICENSE).

## Acknowledgments

- [OpenRouter](https://openrouter.ai) for their API.
- The Rust community and open source contributors for the libraries used in this project.
```
