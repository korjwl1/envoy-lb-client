fn input_widget_builder<'a>(app: &'a mut App, index: usize, title: &str, mode: InputMode) -> Paragraph<'a> {
    let text = if index == 0 {app.dst_url.as_str()} 
                else if index == 1 {app.delay_ms.as_str()} 
                else if index == 2 {app.header_size_kb.as_str()}
                else {app.iteration.as_str()};

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

pub fn ui(f: &mut Frame, app: &mut App) {
    // 메인 레이아웃 분할 (상단 입력 영역, 하단 로그 영역)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(12), // 입력 영역
            Constraint::Min(3),   // 로그 영역
        ])
        .split(f.area());

    // 입력 영역 내부 레이아웃
    let input_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // 주소 입력창
            Constraint::Length(3), // 지연시간, 헤더 크기 입력
            Constraint::Length(3), // 반복 횟수, HTTP 프로토콜 선택
            Constraint::Length(3), // 실행 버튼
        ])
        .split(chunks[0]);
    
    // 주소입력 행
    let dst_url_text = input_widget_builder(app, 0, "Destination URL", InputMode::EditingDstUrl);
    f.render_widget(dst_url_text, input_chunks[0]);

    // 첫 번째 행 (지연시간, 헤더 크기 입력)
    let second_row_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(input_chunks[1]);

    // 두번째 행 (반복 횟수, 프로토콜)
    let third_row_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50)
        ]).split(input_chunks[2]);

    // 지연시간 입력 필드
    let delay_text = input_widget_builder(app, 0, "Delay (ms)", InputMode::EditingDelay);
    f.render_widget(delay_text, second_row_chunks[0]);

    // 헤더 크기 입력 필드
    let header_text = input_widget_builder(app, 1, "Header Size(kb)", InputMode::EditingHeaderSize);
    f.render_widget(header_text, second_row_chunks[1]);

    // 반복 입력 필드
    let iter_text = input_widget_builder(app, 2, "Iteration", InputMode::EditingIteration);
    f.render_widget(iter_text, third_row_chunks[0]);

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
    
    f.render_widget(tabs, third_row_chunks[1]);

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
    
    f.render_widget(button, input_chunks[3]);

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
        InputMode::EditingDstUrl => {
            f.set_cursor_position(Position {
                x: input_chunks[0].x + app.dst_url.len() as u16 + 1,
                y: input_chunks[0].y + 1,
            });
        }
        InputMode::EditingDelay => {
            f.set_cursor_position(Position {
                x: second_row_chunks[0].x + app.delay_ms.len() as u16 + 1,
                y: second_row_chunks[0].y + 1,
            });
        }
        InputMode::EditingHeaderSize => {
            f.set_cursor_position(Position {
                x: second_row_chunks[1].x + app.header_size_kb.len() as u16 + 1,
                y: second_row_chunks[1].y + 1,
            });
        }
        InputMode::EditingIteration => {
            f.set_cursor_position(Position {
                x: third_row_chunks[0].x + app.iteration.len() as u16 + 1,
                y: third_row_chunks[0].y + 1,
            });
        }
        _ => {}
    }
}