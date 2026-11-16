use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, channel};
use std::thread::spawn;

#[derive(Debug, Clone, Copy)]
pub enum Reply {
    Answer(&'static str),
    HangUp,
}

pub fn serve(replies: Vec<Reply>) -> (String, Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("binds");
    let url = format!("http://{}", listener.local_addr().expect("has an address"));
    let (sender, requests) = channel();

    spawn(move || {
        for reply in replies {
            let (mut stream, _) = listener.accept().expect("accepts");
            let request = read_request(&mut stream);

            if let Reply::Answer(body) = reply {
                answer(&mut stream, &body.replace("ID", &request_id(&request)));
            }
            drop(stream);

            sender.send(request).ok();
        }
    });

    (url, requests)
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut request = Vec::new();
    let mut chunk = [0u8; 4096];

    while !complete(&request) {
        let read = stream.read(&mut chunk).expect("reads");
        assert!(read > 0, "the client hung up mid-request");
        request.extend_from_slice(&chunk[..read]);
    }

    String::from_utf8(request).expect("utf-8")
}

fn complete(request: &[u8]) -> bool {
    let text = String::from_utf8_lossy(request);
    let Some((head, body)) = text.split_once("\r\n\r\n") else {
        return false;
    };

    let length = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())?
        })
        .unwrap_or(0);

    body.len() >= length
}

fn request_id(request: &str) -> String {
    let after = request.split("\"id\":").nth(1).expect("has an id");
    after.chars().take_while(char::is_ascii_digit).collect()
}

fn answer(stream: &mut TcpStream, body: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).expect("writes");
}
