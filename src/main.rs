mod utils;

use std::{io, sync::{mpsc, Arc, Mutex}, thread, time::{Duration, Instant}};
use chrono::Local;
use color_eyre::eyre;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use ratatui::{
    layout::{Constraint, Direction, Layout, Position},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs},
    Frame, Terminal,
};
use utils::{input_handling, input_widget_builder};

// 작업 스레드와 공유할 상태
pub struct AppState {
    running: bool,
    iteration: usize,
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
    EditingDelay,
    EditingHeaderSize,
    EditingIteration
}

struct App {
    // 입력 필드
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
    // 포커스된 항목 (0: 지연시간, 1: 헤더 크기, 2: 반복 횟수, 3: HTTP 프로토콜, 4: 실행 버튼, 5: 로그 영역)
    focused_item: usize,
}

impl Default for App {
    fn default() -> Self {
        Self {
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
    }));
    
    let app_state_clone = app_state.clone();
    
    // 작업 스레드
    thread::spawn(move || {
        let mut iter = 0;
        loop {
            // 상태 확인
            let state = {
                let state = app_state_clone.lock().unwrap();
                (state.running, state.iteration)
            };
            
            let (running, max_iter) = state;
            
            if running && iter < max_iter {
                // 로그 추가
                thread::sleep(Duration::from_millis(500)); // 로그 생성 간격
                
                let mut state = app_state_clone.lock().unwrap();
                state.add_log("요청 실행 중...");
                
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
            }
            
            // 작업 스레드가 너무 CPU를 점유하지 않도록 짧은 대기
            thread::sleep(Duration::from_millis(100));
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
                        app.focused_item = (app.focused_item + 1) % 6; // 로그 영역까지 포함하여 6개 항목
                        match app.focused_item {
                            0 | 1 | 2 | 3 => app.input_mode = InputMode::Normal,
                            _ => {}
                        }
                    }
                    KeyCode::BackTab => {
                        app.focused_item = (app.focused_item + 5) % 6; // 로그 영역까지 포함하여 6개 항목
                        match app.focused_item {
                            0 | 1 | 2 | 3 => app.input_mode = InputMode::Normal,
                            _ => {}
                        }
                    }
                    KeyCode::Enter => match app.focused_item {
                        0 => app.input_mode = InputMode::EditingDelay,
                        1 => app.input_mode = InputMode::EditingHeaderSize,
                        2 => app.input_mode = InputMode::EditingIteration,
                        3 => app.protocol_index = (app.protocol_index + 1) % app.protocols.len(),
                        4 => {
                            // 실행/중지 토글
                            let mut state = app_state.lock().unwrap();
                            state.iteration = app.iteration.parse::<usize>().unwrap_or(1);
                            state.running = !state.running;
                            
                            if state.running {
                                let delay = app.delay_ms.parse::<u64>().unwrap_or(100);
                                let header_size = app.header_size_kb.parse::<usize>().unwrap_or(1);
                                let protocol = app.protocols[app.protocol_index];
                                let iteration = app.iteration.parse::<usize>().unwrap_or(1);
                                        
                                state.add_log(&format!("Process Start: Delay {}ms, Header Size {}kb, Protocol {}, Iter {}", delay, header_size, protocol, iteration));
                            } else {
                                state.add_log("Process Stopped by user");
                            }
                            
                            // 새 로그가 추가되면 자동으로 스크롤을 최신 로그로 이동 (focused_item이 로그 영역일 때만)
                            if app.focused_item == 5 {
                                app.log_scroll = 0;
                            }
                        }
                        _ => {}
                    },
                    KeyCode::Esc => app.input_mode = InputMode::Normal,
                    // 입력 모드에 따라 다른 키 처리
                    key => match app.input_mode {
                        InputMode::EditingDelay => input_handling(&mut app.delay_ms, key),
                        InputMode::EditingHeaderSize => input_handling(&mut app.header_size_kb, key),
                        InputMode::EditingIteration => input_handling(&mut app.iteration, key),
                        InputMode::Normal => match app.focused_item {
                            3 => {
                                if matches!(key, KeyCode::Right | KeyCode::Char('l')) {
                                    app.protocol_index = (app.protocol_index + 1) % app.protocols.len();
                                } else if matches!(key, KeyCode::Left | KeyCode::Char('h')) {
                                    app.protocol_index = (app.protocol_index + app.protocols.len() - 1) % app.protocols.len();
                                }
                            }
                            4 => {
                                if matches!(key, KeyCode::Char(' ')) {
                                    // 실행/중지 토글
                                    let mut state = app_state.lock().unwrap();
                                    state.running = !state.running;
                                    
                                    if state.running {
                                        let delay = app.delay_ms.parse::<u64>().unwrap_or(100);
                                        let header_size = app.header_size_kb.parse::<usize>().unwrap_or(1);
                                        let protocol = app.protocols[app.protocol_index];
                                        let iteration = app.iteration.parse::<usize>().unwrap_or(1);
                                        
                                        state.add_log(&format!("Process Start: Delay {}ms, Header Size {}kb, Protocol {}, Iter {}", delay, header_size, protocol, iteration));
                                    } else {
                                        state.add_log("Process Stopped by user");
                                    }
                                }
                            }
                            5 => {
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

fn ui(f: &mut Frame, app: &mut App) {
    // 메인 레이아웃 분할 (상단 입력 영역, 하단 로그 영역)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(9), // 입력 영역
            Constraint::Min(3),   // 로그 영역
        ])
        .split(f.area());

    // 입력 영역 내부 레이아웃
    let input_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // 지연시간, 헤더 크기 입력
            Constraint::Length(3), // HTTP 프로토콜 선택
            Constraint::Length(3), // 실행 버튼
        ])
        .split(chunks[0]);

    // 첫 번째 행 (지연시간, 헤더 크기 입력)
    let first_row_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(input_chunks[0]);

    // 두번째 행 (반복 횟수, 프로토콜)
    let second_row_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50)
        ]).split(input_chunks[1]);

    // 지연시간 입력 필드
    let delay_text = input_widget_builder(app, 0);
    
    f.render_widget(delay_text, first_row_chunks[0]);

    // 헤더 크기 입력 필드
    let header_text = input_widget_builder(app, 1);
    
    f.render_widget(header_text, first_row_chunks[1]);

    // 반복 입력 필드
    let iter_text = input_widget_builder(app, 2);

    f.render_widget(iter_text, second_row_chunks[0]);

    // HTTP 프로토콜 선택
    let protocol_style = if app.focused_item == 3 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    
    let protocols: Vec<Line> = app
        .protocols
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if i == app.protocol_index {
                Line::from(vec![Span::styled(
                    *p,
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                )])
            } else {
                Line::from(vec![Span::raw(*p)])
            }
        })
        .collect();

    let tabs = Tabs::new(protocols)
        .block(
            Block::default()
                .title("HTTP Protocol")
                .borders(Borders::ALL)
                .border_style(protocol_style),
        )
        .select(app.protocol_index)
        .style(Style::default())
        .highlight_style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD));
    
    f.render_widget(tabs, second_row_chunks[1]);

    // 실행 버튼
    let button_style = if app.focused_item == 4 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let button_text = if app.running { "Stop" } else { "Start" };
    let button_color = if app.running { Color::Red } else { Color::Green };
    
    let button = Paragraph::new(button_text)
        .style(Style::default().fg(button_color).add_modifier(Modifier::BOLD))
        .alignment(ratatui::layout::Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(button_style),
        );
    
    f.render_widget(button, input_chunks[2]);

    // 로그 영역
    let log_style = if app.focused_item == 5 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    
    let visible_height = chunks[1].height as usize - 2; // 테두리 제외 높이
    
    // 표시할 로그 항목 계산
    let logs_count = app.logs.len();
    let start_index = if logs_count > 0 {
        // 스크롤 위치에 따라 시작 인덱스 계산
        logs_count.saturating_sub(visible_height).saturating_sub(app.log_scroll)
    } else {
        0
    };
    
    let end_index = logs_count;
    
    let logs: Vec<ListItem> = app
        .logs
        .iter()
        .skip(start_index)
        .take(end_index - start_index)
        .map(|log| {
            ListItem::new(Line::from(log.to_owned()))
        })
        .collect();

    let log_title = if app.focused_item == 5 {
        format!("Log [{}/{}]", app.log_scroll, logs_count.saturating_sub(1).max(0))
    } else {
        "Log".to_string()
    };

    let logs_list = List::new(logs)
        .block(Block::default()
            .borders(Borders::ALL)
            .title(log_title)
            .border_style(log_style))
        .style(Style::default());
    
    f.render_widget(logs_list, chunks[1]);

    // 커서 위치 (입력 모드일 때만)
    match app.input_mode {
        InputMode::EditingDelay => {
            f.set_cursor_position(Position {
                x: first_row_chunks[0].x + app.delay_ms.len() as u16 + 1,
                y: first_row_chunks[0].y + 1,
            });
        }
        InputMode::EditingHeaderSize => {
            f.set_cursor_position(Position {
                x: first_row_chunks[1].x + app.header_size_kb.len() as u16 + 1,
                y: first_row_chunks[1].y + 1,
            });
        }
        InputMode::EditingIteration => {
            f.set_cursor_position(Position {
                x: second_row_chunks[0].x + app.iteration.len() as u16 + 1,
                y: second_row_chunks[0].y + 1,
            });
        }
        _ => {}
    }
}