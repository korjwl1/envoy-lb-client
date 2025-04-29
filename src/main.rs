use std::{
    io,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

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

// 애플리케이션 상태
enum InputMode {
    Normal,
    EditingDelay,
    EditingHeaderSize,
}

struct App {
    // 입력 필드
    delay_ms: String,
    header_size_kb: String,
    // 선택된 HTTP 프로토콜 (0 = HTTP/1.1, 1 = HTTP/2)
    protocol_index: usize,
    protocols: Vec<&'static str>,
    // 현재 입력 모드
    input_mode: InputMode,
    // 로그 메시지
    logs: Vec<String>,
    // 실행 중 여부
    running: bool,
    // 포커스된 항목 (0: 지연시간, 1: 헤더 크기, 2: HTTP 프로토콜, 3: 실행 버튼)
    focused_item: usize,
}

impl Default for App {
    fn default() -> Self {
        Self {
            delay_ms: String::from("100"),
            header_size_kb: String::from("1"),
            protocol_index: 0,
            protocols: vec!["HTTP/1.1", "HTTP/2"],
            input_mode: InputMode::Normal,
            logs: Vec::new(),
            running: false,
            focused_item: 0,
        }
    }
}

impl App {
    fn add_log(&mut self, message: &str) {
        let timestamp = Local::now().format("%H:%M:%S").to_string();
        self.logs.push(format!("[{}] {}", timestamp, message));
    }

    fn run_task(&mut self) {
        if self.running {
            return;
        }

        self.running = true;
        
        // 입력값 파싱
        let delay = self.delay_ms.parse::<u64>().unwrap_or(100);
        let header_size = self.header_size_kb.parse::<usize>().unwrap_or(1);
        let protocol = self.protocols[self.protocol_index];
        
        self.add_log(&format!(
            "Program Start: Delay {}ms, Header Size {}kb, Protocol {}",
            delay, header_size, protocol
        ));
        
        // 여기서 실제로 HTTP 요청을 보내는 로직을 구현할 수 있습니다.
        // 이 예제에서는 간단히 로그만 출력합니다.
    }
    
    fn stop_task(&mut self) {
        if !self.running {
            return;
        }
        
        self.running = false;
        self.add_log("작업 중단됨");
    }
}

fn main() -> Result<(), io::Error> {
    // 터미널 설정
    enable_raw_mode()?; // 버퍼링 과정 없이 터미널에 입력된 값을 바로바로 프로그램에 전달하겠다는 의미
    let mut stdout = io::stdout(); 
    // EnterAlternateScreen: 대체 스크린 버퍼로 전환 -> Nano 같은거 들어갈 때처럼 새 화면 제공 후 끝나면 멀쩡하게
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

    // 키 입력을 비동기로 수신하고 메인 스레드가 자유롭게 UI를 그릴 수 있게 하기 위한 코드
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
        // UI 그리기
        terminal.draw(|f| ui(f, &mut app))?;

        // 이벤트 처리
        match rx.recv()? {
            KeyCode::Char('q') => {
                app.stop_task();
                return Ok(());
            }
            KeyCode::Tab => {
                app.focused_item = (app.focused_item + 1) % 4;
                match app.focused_item {
                    0 | 1 | 2 => app.input_mode = InputMode::Normal,
                    _ => {}
                }
            }
            KeyCode::BackTab => {
                app.focused_item = (app.focused_item + 3) % 4;
                match app.focused_item {
                    0 | 1 | 2 => app.input_mode = InputMode::Normal,
                    _ => {}
                }
            }
            KeyCode::Enter => match app.focused_item {
                0 => app.input_mode = InputMode::EditingDelay,
                1 => app.input_mode = InputMode::EditingHeaderSize,
                2 => app.protocol_index = (app.protocol_index + 1) % app.protocols.len(),
                3 => {
                    if app.running {
                        app.stop_task();
                    } else {
                        app.run_task();
                    }
                }
                _ => {}
            },
            KeyCode::Esc => app.input_mode = InputMode::Normal,
            // 입력 모드에 따라 다른 키 처리
            key => match app.input_mode {
                InputMode::EditingDelay => input_handling(&mut app.delay_ms, key),
                InputMode::EditingHeaderSize => input_handling(&mut app.header_size_kb, key),
                InputMode::Normal => match app.focused_item {
                    2 => {
                        if matches!(key, KeyCode::Right | KeyCode::Char('l')) {
                            app.protocol_index = (app.protocol_index + 1) % app.protocols.len();
                        } else if matches!(key, KeyCode::Left | KeyCode::Char('h')) {
                            app.protocol_index = (app.protocol_index + app.protocols.len() - 1) % app.protocols.len();
                        }
                    }
                    3 => {
                        if matches!(key, KeyCode::Char(' ')) {
                            if app.running {
                                app.stop_task();
                            } else {
                                app.run_task();
                            }
                        }
                    }
                    _ => {}
                },
            },
        }

        // 실행 중인 경우 주기적으로 로그 업데이트
        if app.running {
            let delay = app.delay_ms.parse::<u64>().unwrap_or(100);
            if delay > 0 && app.logs.len() < 100 { // 로그가 너무 많아지지 않도록 제한
                thread::sleep(Duration::from_millis(delay));
                let header_size = app.header_size_kb.parse::<usize>().unwrap_or(1);
                let protocol = app.protocols[app.protocol_index];
                app.add_log(&format!(
                    "요청 완료: {}kb 헤더 전송 ({})",
                    header_size, protocol
                ));
            }
        }
    }
}

fn input_handling(input: &mut String, key: KeyCode) {
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

    // 지연시간 입력 필드
    let delay_style = if app.focused_item == 0 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    
    let delay_block = Block::default()
        .borders(Borders::ALL)
        .title("Delay (ms)")
        .border_style(delay_style);
    
    let delay_text = Paragraph::new(app.delay_ms.as_str())
        .block(delay_block)
        .style(match app.input_mode {
            InputMode::EditingDelay => Style::default().fg(Color::Yellow),
            _ => Style::default(),
        });
    
    f.render_widget(delay_text, first_row_chunks[0]);

    // 헤더 크기 입력 필드
    let header_style = if app.focused_item == 1 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    
    let header_block = Block::default()
        .borders(Borders::ALL)
        .title("Header Size (kb)")
        .border_style(header_style);
    
    let header_text = Paragraph::new(app.header_size_kb.as_str())
        .block(header_block)
        .style(match app.input_mode {
            InputMode::EditingHeaderSize => Style::default().fg(Color::Yellow),
            _ => Style::default(),
        });
    
    f.render_widget(header_text, first_row_chunks[1]);

    // HTTP 프로토콜 선택
    let protocol_style = if app.focused_item == 2 {
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
    
    f.render_widget(tabs, input_chunks[1]);

    // 실행 버튼
    let button_style = if app.focused_item == 3 {
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
    let logs: Vec<ListItem> = app
        .logs
        .iter()
        .rev() // 최신 로그를 위에 표시
        .map(|log| {
            ListItem::new(Line::from(log.to_owned()))
        })
        .collect();

    let logs_list = List::new(logs)
        .block(Block::default().borders(Borders::ALL).title("로그"))
        .style(Style::default());
    
    f.render_widget(logs_list, chunks[1]);

    // 커서 위치 (입력 모드일 때만)
    match app.input_mode {
        InputMode::EditingDelay => {
            f.set_cursor_position(
                Position {x: first_row_chunks[0].x + app.delay_ms.len() as u16 + 1,
                y: first_row_chunks[0].y + 1}
            );
        }
        InputMode::EditingHeaderSize => {
            f.set_cursor_position(
                Position {x: first_row_chunks[1].x + app.header_size_kb.len() as u16 + 1,
                y: first_row_chunks[1].y + 1}
            );
        }
        _ => {}
    }
}