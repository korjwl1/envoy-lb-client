use std::{sync::{Arc, Mutex}, time::Duration};

use crossterm::event::KeyCode;
use rand::{distr::Alphanumeric, Rng};
use reqwest::{header::{HeaderMap, HeaderValue}, Client, Version};

use crate::AppState;

fn random_string(size: usize) -> String {
    rand::rng().sample_iter(&Alphanumeric).take(size * 1024).map(char::from).collect::<String>()
}

fn create_header(id: &str, size: usize) -> HeaderMap {
    // 헤더 생성
    let mut headers = HeaderMap::new();
    headers.insert("my_id", HeaderValue::from_str(id).unwrap());
    headers.insert("random_header", HeaderValue::from_str(
        &random_string(size)
    ).expect("Failed to add random header"));
    headers
}

pub async fn send_request(url: &str, header_size: usize, http_v: &str, state: Arc<Mutex<AppState>>) -> reqwest::Result<()> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .tcp_keepalive(Duration::from_secs(60)).tcp_nodelay(true)
        .pool_max_idle_per_host(5).pool_idle_timeout(Duration::from_secs(90))
        .http1_only().build()?.post(url);
    
    // HTTP Request 보내기
    let random_bytes: [u8; 8] = rand::rng().random();
    let my_id = base62::encode(u64::from_be_bytes(random_bytes));
    let headers = create_header(&my_id, header_size);

    let sender = if http_v == "queryString" {
        client.query(&[("content", &random_string(header_size))])
    } else {
        client.headers(headers)
    };

    let result_log = match sender.send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                match response.text().await {
                    Ok(_) => format!("Request {} Succeded", &my_id),
                    Err(e) => format!("Response {} Failed. HTTP {}: {}", &my_id, &status, e)
                }
            }
            else {
                format!("Request {} Failed. HTTP {}", &my_id, &status)
            }
        }
        Err(e) => format!("Request {} failed to send with error: {}", &my_id, e)
    };

    let mut app_state = state.lock().unwrap();
    app_state.add_log(&result_log);

    drop(app_state);

    Ok(())
}

pub fn input_handling_num(input: &mut String, key: KeyCode) {
    match key {
        KeyCode::Char(c) => {
            if c.is_digit(10) {
                input.push(c);
            }
        }
        KeyCode::Backspace => {
            input.pop();
        }
        _ => {}
    }
}

pub fn input_handling(input: &mut String, key: KeyCode) {
    match key {
        KeyCode::Char(c) => {
            input.push(c);
        }
        KeyCode::Backspace => {
            input.pop();
        }
        _ => {}
    }
}