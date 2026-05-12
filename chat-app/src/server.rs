use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio_stream::{wrappers::UnboundedReceiverStream, Stream};
use tonic::{transport::Server, Request, Response, Status};

use simple_chat::{
    chat_service_server::{ChatService, ChatServiceServer},
    ChatMessage, JoinRequest, JoinResponse, LeaveRequest, LeaveResponse, MessageRequest,
    MessageResponse, StreamRequest,
};

type MessageSender = mpsc::UnboundedSender<Result<ChatMessage, Status>>;

#[derive(Debug)]
struct ChatRoom {
    users: HashMap<String, MessageSender>,
}

impl ChatRoom {
    fn new() -> Self {
        Self {
            users: HashMap::new(),
        }
    }

    fn add_user(&mut self, username: String, sender: MessageSender) -> bool {
        if self.users.contains_key(&username) {
            false // Username already exists
        } else {
            self.users.insert(username, sender);
            true
        }
    }

    fn remove_user(&mut self, username: &str) -> bool {
        self.users.remove(username).is_some()
    }

    async fn broadcast_message(&self, sender_username: &str, message: String) {
        let timestamp = chrono::Utc::now().timestamp();
        let chat_message = ChatMessage {
            username: sender_username.to_string(),
            message,
            timestamp,
        };

        // Send to all users except the sender
        for (username, sender) in &self.users {
            if username != sender_username {
                let _ = sender.send(Ok(chat_message.clone()));
            }
        }
    }
}

#[derive(Debug)]
pub struct ChatServiceImpl {
    chat_room: Arc<RwLock<ChatRoom>>,
}

impl ChatServiceImpl {
    pub fn new() -> Self {
        Self {
            chat_room: Arc::new(RwLock::new(ChatRoom::new())),
        }
    }
}

#[tonic::async_trait]
impl ChatService for ChatServiceImpl {
    async fn join(&self, request: Request<JoinRequest>) -> Result<Response<JoinResponse>, Status> {
        let req = request.into_inner();
        let username = req.username;

        if username.trim().is_empty() {
            return Ok(Response::new(JoinResponse {
                success: false,
                message: "Username cannot be empty".to_string(),
            }));
        }

        // We don't add the user here yet, that happens in stream_messages
        // This is just to validate the username format
        let chat_room = self.chat_room.read().await;
        if chat_room.users.contains_key(&username) {
            Ok(Response::new(JoinResponse {
                success: false,
                message: "Username already taken".to_string(),
            }))
        } else {
            Ok(Response::new(JoinResponse {
                success: true,
                message: "Username available".to_string(),
            }))
        }
    }

    async fn send_message(
        &self,
        request: Request<MessageRequest>,
    ) -> Result<Response<MessageResponse>, Status> {
        let req = request.into_inner();
        let username = req.username;
        let message = req.message;

        if message.trim().is_empty() {
            return Ok(Response::new(MessageResponse {
                success: false,
                message: "Message cannot be empty".to_string(),
            }));
        }

        let chat_room = self.chat_room.read().await;

        if !chat_room.users.contains_key(&username) {
            return Ok(Response::new(MessageResponse {
                success: false,
                message: "User not connected".to_string(),
            }));
        }

        chat_room.broadcast_message(&username, message).await;

        Ok(Response::new(MessageResponse {
            success: true,
            message: "Message sent".to_string(),
        }))
    }

    async fn leave(
        &self,
        request: Request<LeaveRequest>,
    ) -> Result<Response<LeaveResponse>, Status> {
        let req = request.into_inner();
        let username = req.username;

        let mut chat_room = self.chat_room.write().await;
        let removed = chat_room.remove_user(&username);

        Ok(Response::new(LeaveResponse {
            success: removed,
            message: if removed {
                "User left successfully".to_string()
            } else {
                "User not found".to_string()
            },
        }))
    }

    type StreamMessagesStream = Pin<Box<dyn Stream<Item = Result<ChatMessage, Status>> + Send>>;

    async fn stream_messages(
        &self,
        request: Request<StreamRequest>,
    ) -> Result<Response<Self::StreamMessagesStream>, Status> {
        let req = request.into_inner();
        let username = req.username;

        if username.trim().is_empty() {
            return Err(Status::invalid_argument("Username cannot be empty"));
        }

        let (tx, rx) = mpsc::unbounded_channel();

        {
            let mut chat_room = self.chat_room.write().await;
            if !chat_room.add_user(username.clone(), tx) {
                return Err(Status::already_exists("Username already taken"));
            }
        }

        println!("User {} joined the chat", username);

        // Create the stream
        let output_stream = UnboundedReceiverStream::new(rx);
        Ok(Response::new(
            Box::pin(output_stream) as Self::StreamMessagesStream
        ))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:50051".parse()?;
    let chat_service = ChatServiceImpl::new();

    println!("Chat server listening on {}", addr);

    Server::builder()
        .add_service(ChatServiceServer::new(chat_service))
        .serve(addr)
        .await?;

    Ok(())
}
