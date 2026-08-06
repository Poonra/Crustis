mod protocol;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

//           bound. Gives shared access but NO mutability.
//   Mutex — one writer at a time. Gives mutability but only one owner.

type Store = Arc<Mutex<HashMap<Vec<u8>, Vec<u8>>>>;

const MAX_BUF: usize = 8 * 1024 * 1024;
fn main() {
    let listener = TcpListener::bind("127.0.0.1:6379").expect("Could not bind 127.0.0.1:6379");
    let store: Store = Arc::new(Mutex::new(HashMap::new()));

    println!("Listening on 127.0.0.1:6379");

    loop {
        // accept() does NOT create a connection — it hands the kernel
        // already established. It BLOCKS until one is available (the thread
        // parks in the kernel, burning no CPU).
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
    let mut buf = Vec::new();
    let mut chunk = [0u8; 512];

    loop {
        let n = match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                eprintln!("read error : {e}");
                break;
            }
        };

        buf.extend_from_slice(&chunk[..n]); // only ..n is valid after is stale

        if buf.len() > MAX_BUF {
            let _ = stream.write_all(b"-ERR protocol error: input too large\r\n");
            break;
        }

        loop {
            match protocol::parse(&buf) {
                Ok(Some((args, consumed))) => {
                    let reply = handle_command(&args, &store);

                    buf.drain(..consumed);

                    if stream.write_all(&reply).is_err() {
                        return;
                    }
                }

                Ok(None) => break,

                Err(e) => {
                    let _ = stream.write_all(format!("-ERR protocol error: {e}\r\n").as_bytes());
                    return;
                }
            }
        }
    }
    println!("client disconnected");
}

fn handle_command(args: &[Vec<u8>], store: &Store) -> Vec<u8> {
    let Some(cmd) = args.first() else {
        return b"-ERR empty command\r\n".to_vec();
    };

    match cmd.to_ascii_uppercase().as_slice() {
        b"PING" => b"+PONG\r\n".to_vec(),

        b"SET" if args.len() == 3 => {
            let mut map = store.lock().expect("store lock poisoned");
            map.insert(args[1].clone(), args[2].clone()); // must own; args dies with this call
            b"+OK\r\n".to_vec()
        }
        b"SET" => wrong_args("set"),

        b"GET" if args.len() == 2 => {
            let map = store.lock().expect("store lock poisoned");
            match map.get(&args[1]) {
                Some(v) => protocol::bulk(v),
                None => b"$-1\r\n".to_vec(),
            }
        }
        b"GET" => wrong_args("get"),

        b"DEL" if args.len() == 2 => {
            let mut map = store.lock().expect("store lock poisoned");
            // :N\r\n — RESP integer. Redis returns the number of keys actually removed.
            if map.remove(&args[1]).is_some() {
                b":1\r\n".to_vec()
            } else {
                b":0\r\n".to_vec()
            }
        }

        b"DEL" => wrong_args("del"),

        _ => {
            let name: String = String::from_utf8_lossy(cmd)
                .chars()
                .filter(|c| !c.is_control())
                .take(32)
                .collect();
            format!("-ERR unknown command '{name}'\r\n").into_bytes()
        }
    }
}

fn wrong_args(name: &str) -> Vec<u8> {
    format!("-ERR wrong number of arguments for '{name}' command\r\n").into_bytes()
}
