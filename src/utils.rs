use std::sync::{Arc, Mutex};

use crossterm::event::KeyCode;
use rand::{distr::Alphanumeric, Rng};
use ratatui::{style::{Color, Style}, widgets::{Block, Borders, Paragraph}};
use reqwest::{header::{HeaderMap, HeaderValue}, Client};

use crate::{App, AppState, InputMode};

const SERVER_URL: &str = "http://localhost:8020";

fn create_header(id: &str, size: usize) -> HeaderMap {
    // 헤더 생성
    let mut headers = HeaderMap::new();
    headers.insert("my_id", HeaderValue::from_str(id).unwrap());
    headers.insert("random_header", HeaderValue::from_str(
        &rand::rng().sample_iter(&Alphanumeric).take(size * 1024).map(char::from).collect::<String>()
    ).expect("Failed to add random header"));
    headers
}

pub async fn send_request(header_size: &str, http_v: &str, state: Arc<Mutex<AppState>>) -> reqwest::Result<()> {
    // 클라이언트 생성
    let cb = if http_v == "HTTP/1.1" {Client::builder().http1_only()} else {Client::builder().http2_prior_knowledge()};
    let client = cb.build()?;
    
    // 헤더 사이즈 String -> usize로 변환
    let header_size = header_size.parse::<usize>().unwrap_or(1);

    // HTTP Request 보내기
    let random_bytes: [u8; 8] = rand::rng().random();
    let my_id = base62::encode(u64::from_be_bytes(random_bytes));
    let headers = create_header(&my_id, header_size);


    let result_log = match client.post(SERVER_URL).headers(headers).send().await {
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
        Err(e) => format!("Request {} failed to get send with error: {}", &my_id, e)
    };

    let mut app_state = state.lock().unwrap();
    app_state.add_log(&result_log);

    drop(app_state);

    Ok(())
}

pub fn input_widget_builder<'a>(app: &'a mut App, index: usize) -> Paragraph<'a> {
    let title = if index == 0 {"Delay (ms)"} else if index == 1 {"Header Size(kb)"} else {"Iteration"};
    let text = if index == 0 {app.delay_ms.as_str()} else if index == 1 {app.header_size_kb.as_str()} else {app.iteration.as_str()};
    let mode = if index == 0 {InputMode::EditingDelay} else if index == 1 {InputMode::EditingHeaderSize} else {InputMode::EditingIteration};

    let delay_style = if app.focused_item == index {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    
    let delay_block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(delay_style);
    
    let delay_text = Paragraph::new(text)
        .block(delay_block)
        .style(
            if app.input_mode == mode { Style::default().fg(Color::Yellow) } else { Style::default() }
        );

    return delay_text;
}

pub fn input_handling(input: &mut String, key: KeyCode) {
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