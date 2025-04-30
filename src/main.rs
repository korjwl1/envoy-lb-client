mod utils;
mod ui;

use std::{io, sync::{mpsc, Arc, Mutex}, thread, time::{Duration, Instant}};
use chrono::Local;
use color_eyre::eyre;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};


use ratatui::Terminal;
use utils::*;
use ui::ui;

// 작업 스레드와 공유할 상태
pub struct AppState {
    running: bool,
    // 실행값
    iteration: usize,
    dst_url: String,
    delay_ms: u64,
    header_size_kb: usize,
    protocol: String,
    // 로그
    logs: Vec<String>,
}

impl AppState {
    pub fn add_log(&mut self, log: &str) {
        let timestamp = Local::now().format("%H:%M:%S%.6f").to_string();
        self.logs.push(format!("[{}] {}", timestamp, log));

        if self.logs.len() > 3000 {
            let excess = self.logs.len() - 3000;
            self.logs.drain(0..excess);
        }
    }
}

// 애플리케이션 상태
#[derive(PartialEq, Eq)]
enum InputMode {
    Normal,
    EditingDstUrl,
    EditingDelay,
    EditingHeaderSize,
    EditingIteration
}

struct App {
    // 입력 필드
    dst_url: String,
    delay_ms: String,
    header_size_kb: String,
    iteration: String,
    // 선택된 HTTP 프로토콜 (0 = HTTP/1.1, 1 = HTTP/2)
    protocol_index: usize,
    protocols: Vec<&'static str>,
    // 현재 입력 모드
    input_mode: InputMode,
    // 로그 메시지
    logs: Vec<String>,
    // 로그 스크롤 위치
    log_scroll: usize,
    // 실행 중 여부
    running: bool,
    // 포커스된 항목 (0: 주소입력창, 1: 지연시간, 2: 헤더 크기, 3: 반복 횟수, 4: HTTP 프로토콜, 5: 실행 버튼, 6: 로그 영역)
    focused_item: usize,
}

impl Default for App {
    fn default() -> Self {
        Self {
            dst_url: String::from(""),
            delay_ms: String::from("100"),
            header_size_kb: String::from("1"),
            iteration: String::from("1"),
            protocol_index: 0,
            protocols: vec!["HTTP/1.1", "HTTP/2"],
            input_mode: InputMode::Normal,
            logs: Vec::new(),
            log_scroll: 0,
            running: false,
            focused_item: 0,
        }
    }
}


fn main() -> Result<(), io::Error> {
    // 터미널 설정
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 애플리케이션 상태 생성
    let app = App::default();
    let res = run_app(&mut terminal, app);

    // 터미널 복원
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
) -> eyre::Result<()> {
    // 이벤트 처리를 위한 설정
    let (tx, rx) = mpsc::channel();
    let tick_rate = Duration::from_millis(100);
    
    // 작업 스레드와 공유할 앱 상태
    let app_state = Arc::new(Mutex::new(AppState {
        running: false,
        iteration: 1,
        logs: Vec::new(),
        dst_url: String::from(""),
        delay_ms: 0,
        header_size_kb: 0,
        protocol: "HTTP/1.1".to_owned(),
    }));
    
    let app_state_clone = app_state.clone();
    
    // 작업 스레드
    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
        let mut iter = 0;

        loop {
            // 상태 확인
            let state = {
                let state = app_state_clone.lock().unwrap();
                (state.running, state.iteration, state.dst_url.clone(), state.delay_ms, state.header_size_kb, state.protocol.clone())
            };
            
            let (running, max_iter, dst_url, delay, header_size, protocol) = state;
            let cloned_app_state = app_state_clone.clone();

            if running && iter < max_iter {
                // 로그 추가
                thread::sleep(Duration::from_millis(delay)); // 로그 생성 간격
                rt.spawn(async move {
                    let _ = send_request(&dst_url, header_size, &protocol, cloned_app_state).await;
                });

                iter = iter + 1;
            }
            else if running {
                let mut state = app_state_clone.lock().unwrap();
                state.running = !state.running;
                state.add_log("Process Done");
                drop(state);
            }
            else {
                iter = 0;
                // 작업 스레드가 너무 CPU를 점유하지 않도록 짧은 대기
                thread::sleep(Duration::from_millis(100));
            }
        }
    });
    
    thread::spawn(move || {
        let mut last_tick = Instant::now();
        loop {
            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));

            if event::poll(timeout).unwrap() {
                if let Event::Key(key) = event::read().unwrap() {
                    if key.kind == KeyEventKind::Press {
                        tx.send(key.code).unwrap();
                    }
                }
            }

            if last_tick.elapsed() >= tick_rate {
                last_tick = Instant::now();
            }
        }
    });

    // 메인 루프
    loop {
        // 작업 스레드에서 로그 업데이트 가져오기
        {
            let state = app_state.lock().unwrap();
            app.logs = state.logs.clone();
            app.running = state.running;
        }
        
        // UI 그리기
        terminal.draw(|f| ui(f, &mut app))?;

        // 이벤트 처리
        match rx.try_recv() {
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => { return Ok(()) }
            Ok(key) => {
                match key {
                    KeyCode::Char('q') => {
                        // 작업 중지 및 종료
                        let mut state = app_state.lock().unwrap();
                        state.running = false;
                        return Ok(());
                    }
                    KeyCode::Tab => {
                        app.focused_item = (app.focused_item + 1) % 7; // 로그 영역까지 포함하여 6개 항목
                        match app.focused_item {
                            0 | 1 | 2 | 3 | 4 => app.input_mode = InputMode::Normal,
                            _ => {}
                        }
                    }
                    KeyCode::BackTab => {
                        app.focused_item = (app.focused_item + 6) % 7; // 로그 영역까지 포함하여 6개 항목
                        match app.focused_item {
                            0 | 1 | 2 | 3 | 4 => app.input_mode = InputMode::Normal,
                            _ => {}
                        }
                    }
                    KeyCode::Enter => match app.focused_item {
                        0 => app.input_mode = InputMode::EditingDstUrl,
                        1 => app.input_mode = InputMode::EditingDelay,
                        2 => app.input_mode = InputMode::EditingHeaderSize,
                        3 => app.input_mode = InputMode::EditingIteration,
                        4 => app.protocol_index = (app.protocol_index + 1) % app.protocols.len(),
                        5 => {
                            // 실행/중지 토글
                            let mut state = app_state.lock().unwrap();

                            if !state.running {
                                let delay = app.delay_ms.parse::<u64>().unwrap_or(100);
                                let header_size = app.header_size_kb.parse::<usize>().unwrap_or(1);
                                let protocol = app.protocols[app.protocol_index];
                                let iteration = app.iteration.parse::<usize>().unwrap_or(1);

                                state.dst_url = app.dst_url.clone();
                                state.delay_ms = delay;
                                state.header_size_kb = header_size;
                                state.iteration = iteration;
                                state.running = true;

                                state.add_log(&format!("Process Start: Delay {}ms, Header Size {}kb, Protocol {}, Iter {}", delay, header_size, protocol, iteration));
                            } else {
                                state.running = false;
                                state.add_log("Process Stopped by user");
                            }
                            
                            // 새 로그가 추가되면 자동으로 스크롤을 최신 로그로 이동 (focused_item이 로그 영역일 때만)
                            if app.focused_item == 6 {
                                app.log_scroll = 0;
                            }
                        }
                        _ => {}
                    },
                    KeyCode::Esc => app.input_mode = InputMode::Normal,
                    // 입력 모드에 따라 다른 키 처리
                    key => match app.input_mode {
                        InputMode::EditingDstUrl => input_handling(&mut app.dst_url, key),
                        InputMode::EditingDelay => input_handling_num(&mut app.delay_ms, key),
                        InputMode::EditingHeaderSize => input_handling_num(&mut app.header_size_kb, key),
                        InputMode::EditingIteration => input_handling_num(&mut app.iteration, key),
                        InputMode::Normal => match app.focused_item {
                            4 => {
                                if matches!(key, KeyCode::Right | KeyCode::Char('l')) {
                                    app.protocol_index = (app.protocol_index + 1) % app.protocols.len();
                                } else if matches!(key, KeyCode::Left | KeyCode::Char('h')) {
                                    app.protocol_index = (app.protocol_index + app.protocols.len() - 1) % app.protocols.len();
                                }
                            }
                            5 => {
                                if matches!(key, KeyCode::Char(' ')) {
                                    // 실행/중지 토글
                                    let mut state = app_state.lock().unwrap();
                                    
                                    if !state.running {
                                        let delay = app.delay_ms.parse::<u64>().unwrap_or(100);
                                        let header_size = app.header_size_kb.parse::<usize>().unwrap_or(1);
                                        let protocol = app.protocols[app.protocol_index];
                                        let iteration = app.iteration.parse::<usize>().unwrap_or(1);

                                        state.dst_url = app.dst_url.clone();
                                        state.delay_ms = delay;
                                        state.header_size_kb = header_size;
                                        state.iteration = iteration;
                                        state.running = true;

                                        state.add_log(&format!("Process Start: Delay {}ms, Header Size {}kb, Protocol {}, Iter {}", delay, header_size, protocol, iteration));
                                    } else {
                                        state.running = false;
                                        state.add_log("Process Stopped by user");
                                    }
                                }
                            }
                            6 => {
                                // 로그 영역 스크롤 처리
                                if matches!(key, KeyCode::Down | KeyCode::Char('j')) {
                                    if app.log_scroll < app.logs.len().saturating_sub(1) {
                                        app.log_scroll += 1;
                                    }
                                } else if matches!(key, KeyCode::Up | KeyCode::Char('k')) {
                                    if app.log_scroll > 0 {
                                        app.log_scroll -= 1;
                                    }
                                } else if matches!(key, KeyCode::PageDown) {
                                    app.log_scroll = (app.log_scroll + 10).min(app.logs.len().saturating_sub(1));
                                } else if matches!(key, KeyCode::PageUp) {
                                    app.log_scroll = app.log_scroll.saturating_sub(10);
                                } else if matches!(key, KeyCode::Home) {
                                    app.log_scroll = 0;
                                } else if matches!(key, KeyCode::End) {
                                    app.log_scroll = app.logs.len().saturating_sub(1);
                                }
                            }
                            _ => {}
                        },
                    },
                }
            }
        }
    }
}