use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

type Store = Arc<Mutex<HashMap<String, String>>>;
fn main() {
    let listener = TcpListener::bind("127.0.0.1:6379").expect("Could not bind 127.0.0.1:6379");
    let store: Store = Arc::new(Mutex::new(HashMap::new()));

    println!("Listening on 127.0.0.1:6379");

    loop {
        match listener.accept() {
            Ok((stream, addr)) => {
                println!("Accepted a client from {}", addr);
                let store = Arc::clone(&store);
                thread::spawn(move || handle_client(stream, store));
            }
            Err(e) => {
                println!("Accept error: {}", e);
            }
        }
    }
}

fn handle_client(mut stream: TcpStream, store: Store) {
    let mut buf = [0u8; 512];

    loop {
        match stream.read(&mut buf) {
            Ok(0) => {
                println!("Client disconnected");
                break;
            }
            Ok(n) => {
                println!("Read {} bytes", n);
                let line = String::from_utf8_lossy(&buf[..n]);
                let response = handle_command(line.trim(), &store);
                if stream.write_all(response.as_bytes()).is_err() {
                    break;
                }
            }
            Err(e) => {
                eprintln!("read error: {}", e);
                break;
            }
        }
    }
}

fn handle_command(line: &str, store: &Store) -> String {
    let mut parts = line.split_whitespace();

    match parts.next() {
        None => String::new(),

        Some(cmd) if cmd.eq_ignore_ascii_case("SET") => match (parts.next(), parts.next()) {
            (Some(key), Some(value)) => {
                let mut map = store.lock().expect("store lock poisoned");
                map.insert(key.to_string(), value.to_string());
                "OK\n".to_string()
            }
            _ => "ERR wrong number of arguments for 'set'\n".to_string(),
        },

        Some(cmd) if cmd.eq_ignore_ascii_case("GET") => match parts.next() {
            Some(key) => {
                let map = store.lock().expect("store lock poisoned");
                match map.get(key) {
                    Some(value) => format!("{value}\n"),
                    None => "(nil)\n".to_string(),
                }
            }
            None => "ERR wrong number of arguments for 'get'\n".to_string(),
        },

        Some(cmd) if cmd.eq_ignore_ascii_case("DEL") => match parts.next() {
            Some(key) => {
                let mut map = store.lock().expect("store lock poisoned");
                if map.remove(key).is_some() {
                    "1\n".to_string()
                } else {
                    "0\n".to_string()
                }
            }
            None => "ERR wrong number of arguments for 'del'\n".to_string(),
        },

        Some(cmd) => format!("ERR unknown command '{cmd}'\n"),
    }
}
