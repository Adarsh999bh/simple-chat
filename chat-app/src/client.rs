use clap::{parser::ValueSource, CommandFactory, Parser};
use futures_util::StreamExt;
use std::env;
use std::io::{self, Write};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;
use tonic::transport::Channel;

use simple_chat::{
    chat_service_client::ChatServiceClient, JoinRequest, LeaveRequest, MessageRequest,
    StreamRequest,
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct CliArgs {
    /// Server ip
    #[arg(short, long, env = "CHAT_IP", default_value = "127.0.0.1")]
    ip: String,

    /// Server port
    #[arg(short, long, env = "CHAT_PORT", default_value = "50051")]
    port: String,

    /// Username for the chat
    #[arg(short, long, env = "CHAT_USERNAME")]
    username: String,
}

struct ChatClient {
    client: ChatServiceClient<Channel>,
    username: String,
}

impl ChatClient {
    async fn new(
        ip: String,
        port: String,
        username: String,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let addr = format!("http://{}:{}", ip, port);
        let client = ChatServiceClient::connect(addr).await?;

        Ok(Self { client, username })
    }

    async fn join(&mut self) -> Result<bool, Box<dyn std::error::Error>> {
        let request = tonic::Request::new(JoinRequest {
            username: self.username.clone(),
        });

        let response = self.client.join(request).await?;
        let join_response = response.into_inner();

        if !join_response.success {
            eprintln!("Failed to join: {}", join_response.message);
            return Ok(false);
        }

        println!("Successfully validated username. Connecting to chat...");
        Ok(true)
    }

    async fn send_message(&mut self, message: String) -> Result<(), Box<dyn std::error::Error>> {
        let request = tonic::Request::new(MessageRequest {
            username: self.username.clone(),
            message,
        });

        let response = self.client.send_message(request).await?;
        let message_response = response.into_inner();

        if !message_response.success {
            println!("Failed to send message: {}", message_response.message);
        }

        Ok(())
    }

    async fn leave(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let request = tonic::Request::new(LeaveRequest {
            username: self.username.clone(),
        });

        let response = self.client.leave(request).await?;
        let leave_response = response.into_inner();

        if leave_response.success {
            println!("Left the chat successfully");
        } else {
            println!("Failed to leave: {}", leave_response.message);
        }

        Ok(())
    }

    async fn start_message_stream(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let request = tonic::Request::new(StreamRequest {
            username: self.username.clone(),
        });

        let response = self.client.stream_messages(request).await?;
        let mut stream = response.into_inner();

        println!("Connected to chat! Type 'help' for commands.");

        // Channel for communication between input handler and main loop
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();

        // Spawn task to handle user input
        let input_tx = tx.clone();
        let input_task = tokio::spawn(async move {
            let mut reader = BufReader::new(tokio::io::stdin());
            let mut line = String::new();

            loop {
                print!("> ");
                let _ = io::stdout().flush();

                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let input = line.trim().to_string();
                        if input_tx.send(input).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // Main event loop
        loop {
            tokio::select! {
                // Handle incoming messages from server
                message = stream.next() => {
                    match message {
                        Some(Ok(chat_message)) => {
                            let timestamp = chrono::DateTime::from_timestamp(chat_message.timestamp, 0)
                                .unwrap_or_else(|| chrono::Utc::now())
                                .format("%H:%M:%S");
                            println!("\r[{}] {}: {}", timestamp, chat_message.username, chat_message.message);
                            print!("> ");
                            let _ = io::stdout().flush();
                        }
                        Some(Err(e)) => {
                            eprintln!("\rError receiving message: {}", e);
                            print!("> ");
                            let _ = io::stdout().flush();
                        }
                        None => {
                            println!("\rConnection to server lost");
                            break;
                        }
                    }
                }
                // Handle user input
                input = rx.recv() => {
                    if let Some(input) = input {
                        if input.is_empty() {
                            continue;
                        }

                        if input == "leave" {
                            self.leave().await?;
                            break;
                        } else if input == "help" {
                            println!("Commands:");
                            println!("  send <message> - Send a message to the chat");
                            println!("  leave          - Leave the chat and exit");
                            println!("  help           - Show this help message");
                        } else if let Some(message) = input.strip_prefix("send ") {
                            if !message.trim().is_empty() {
                                self.send_message(message.to_string()).await?;
                            } else {
                                println!("Usage: send <message>");
                            }
                        } else {
                            println!("Unknown command: '{}'. Type 'help' for available commands.", input);
                        }
                    } else {
                        break;
                    }
                }
            }
        }

        // Clean up
        input_task.abort();
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = CliArgs::parse();
    let cli_args_match = CliArgs::command().get_matches();
    let ip = if let Some(ip) = cli_args_match.value_source("ip") {
        match ip {
            ValueSource::CommandLine => args.ip,
            ValueSource::DefaultValue => match env::var("IP") {
                Ok(ip) => ip,
                Err(_) => {
                    println!("Setting default ip...");
                    args.ip
                }
            },
            _ => args.ip,
        }
    } else {
        args.ip
    };

    let port = if let Some(port) = cli_args_match.value_source("port") {
        match port {
            ValueSource::CommandLine => args.port,
            ValueSource::DefaultValue => match env::var("PORT") {
                Ok(port) => port,
                Err(_) => {
                    println!("Setting default port...");
                    args.port
                }
            },
            _ => args.port,
        }
    } else {
        args.port
    };

    let name = if let Some(name) = cli_args_match.value_source("username") {
        match name {
            ValueSource::CommandLine => args.username,
            ValueSource::DefaultValue => match env::var("NAME") {
                Ok(name) => name,
                Err(_) => {
                    println!("Setting default username...");
                    args.username
                }
            },
            _ => args.username,
        }
    } else {
        args.username
    };

    let mut client = match ChatClient::new(ip.clone(), port.clone(), name.clone()).await {
        Ok(client) => client,
        Err(e) => {
            eprintln!("Failed to connect to server at {}:{}: {}", ip, port, e);
            std::process::exit(1);
        }
    };

    // Validate username with server
    if !client.join().await? {
        std::process::exit(1);
    }

    // Start the message stream and interactive session
    if let Err(e) = client.start_message_stream().await {
        eprintln!("Error during chat session: {}", e);
        std::process::exit(1);
    }

    println!("Goodbye!");
    Ok(())
}
