# Simple Chat - gRPC Implementation

*Video demo*
[working-demo-zip2.zip](https://github.com/user-attachments/files/22197891/working-demo-zip2.zip)


A simple asynchronous chat server and CLI client implemented in Rust using gRPC.

## Running

### Start the Server

```bash
cd chat-app
cargo run --bin server
```

The server will start on `127.0.0.1:50051` by default.

### Start the Client

```bash
# Using command line arguments
cargo run --bin client -- --username adarsh --ip127.0.0.1 --port 50051

# Using environment variables
export USERNAME=adarsh
export IP=127.0.0.1
export PORT=50051
cargo run --bin client
```

## Client Commands

- `send <message>` - Send a message to the chat room
- `leave` - Leave the chat room
- `help` - Show available commands
