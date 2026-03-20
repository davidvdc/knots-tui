use crate::rpc::{BlockInfo, BlockStats, NodeData};
use crate::sys::SystemStats;
use crate::{AnalyticsData, AnalyticsState, Screen};
use ratatui::{prelude::*, widgets::*};
use std::collections::{BTreeMap, HashMap};

pub fn draw(f: &mut Frame, data: &NodeData, peer_scroll: u16, screen: Screen, selected_bit: u8, show_bit_modal: bool, rpc_spinner: u8, block_stats: &HashMap<u64, BlockStats>, selected_block: u16, block_scroll: u16, show_block_modal: bool, blocks_focused: bool, analytics: &AnalyticsData, system_stats: &SystemStats) {
    let area = f.area();

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // header
            Constraint::Min(10),   // body
            Constraint::Length(1), // footer
        ])
        .split(area);

    draw_header(f, outer[0], data, screen);
    match screen {
        Screen::Dashboard => draw_body(f, outer[1], data, peer_scroll, block_stats, selected_block, block_scroll, blocks_focused, analytics, system_stats),
        Screen::KnownPeers => draw_known_peers(f, outer[1], data, peer_scroll),
        Screen::Signaling => draw_signaling(f, outer[1], data, selected_bit),
        Screen::Analytics => draw_analytics(f, outer[1], analytics),
    }
    draw_footer(f, outer[2], screen, rpc_spinner);

    if show_bit_modal && screen == Screen::Signaling {
        draw_bit_modal(f, area, selected_bit, data);
    }
    if show_block_modal && screen == Screen::Dashboard {
        if let Some(b) = data.recent_blocks.get(selected_block as usize) {
            if let Some(s) = block_stats.get(&b.height) {
                draw_block_modal(f, area, b, s);
            }
        }
    }
}

fn draw_header(f: &mut Frame, area: Rect, data: &NodeData, screen: Screen) {
    let version = if !data.network.subversion.is_empty() {
        data.network.subversion.clone()
    } else {
        "connecting...".to_string()
    };

    let chain = if !data.blockchain.chain.is_empty() {
        data.blockchain.chain.clone()
    } else {
        "?".to_string()
    };

    let uptime = format_duration(data.uptime);

    let screen_label = match screen {
        Screen::Dashboard => "Dashboard",
        Screen::KnownPeers => "Known Peers",
        Screen::Signaling => "Signaling",
        Screen::Analytics => "Analytics",
    };

    let title = format!(
        " Bitcoin Knots {} | {} | chain: {} | uptime: {} ",
        screen_label, version, chain, uptime
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().fg(Color::Cyan));

    let header = Paragraph::new(title)
        .block(block)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::White).bold());

    f.render_widget(header, area);
}

fn draw_ibd_screen(f: &mut Frame, area: Rect, data: &NodeData, peer_scroll: u16, sys: &SystemStats) {
    let bc = &data.blockchain;
    let net = &data.network;
    let progress = (bc.verificationprogress * 100.0).min(100.0);

    // Progress bar: 40 chars wide
    let bar_width = 40usize;
    let filled = ((bc.verificationprogress * bar_width as f64) as usize).min(bar_width);
    let bar = format!("[{}{}]", "#".repeat(filled), " ".repeat(bar_width - filled));

    let remaining_blocks = bc.headers.saturating_sub(bc.blocks);
    let eta = if data.ibd_blocks_per_sec > 0.1 {
        let secs = (remaining_blocks as f64 / data.ibd_blocks_per_sec) as u64;
        format!("~{}", format_duration(secs))
    } else {
        "-".to_string()
    };
    let speed = if data.ibd_blocks_per_sec > 0.1 {
        format!("{:.1} blk/s", data.ibd_blocks_per_sec)
    } else {
        "-".to_string()
    };
    let dl_rate = if data.ibd_recv_per_sec > 0 {
        format!("{}/s", format_bytes(data.ibd_recv_per_sec))
    } else {
        "-".to_string()
    };

    let cyan = Style::default().fg(Color::Cyan);
    let gray = Style::default().fg(Color::DarkGray);
    let white = Style::default().fg(Color::White);
    let yellow = Style::default().fg(Color::Yellow).bold();
    let green = Style::default().fg(Color::Green);

    let lines = vec![
        Line::from(vec![
            Span::styled("Progress: ", gray),
            Span::styled(bar, yellow),
            Span::styled(format!("  {:.2}%", progress), yellow),
        ]),
        Line::from(vec![
            Span::styled("Synced:   ", gray),
            Span::styled(format!("{} / {} blocks", format_number(bc.blocks), format_number(bc.headers)), white),
            Span::styled("   tip: ", gray),
            Span::styled(
                if bc.time > 0 {
                    chrono::DateTime::from_timestamp(bc.time as i64, 0)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_else(|| "-".to_string())
                } else {
                    "-".to_string()
                },
                cyan,
            ),
        ]),
        Line::from(vec![
            Span::styled("Speed:    ", gray),
            Span::styled(&speed, cyan),
            Span::styled("   ETA: ", gray),
            Span::styled(&eta, white),
        ]),
        Line::from(vec![
            Span::styled("Peers:    ", gray),
            Span::styled(
                format!("{} (in: {} / out: {})", net.connections, net.connections_in, net.connections_out),
                white,
            ),
        ]),
        Line::from(vec![
            Span::styled("Download: ", gray),
            Span::styled(&dl_rate, green),
            Span::styled("   total recv: ", gray),
            Span::styled(format_bytes(data.net_totals.totalbytesrecv), white),
        ]),
        Line::from(vec![
            Span::styled("Disk:     ", gray),
            Span::styled(format_bytes(bc.size_on_disk), white),
            Span::styled("   Difficulty: ", gray),
            Span::styled(format!("{:.2e}", bc.difficulty), white),
        ]),
    ];

    // System stats box height (1 core per line)
    let has_sys = !sys.cpus.is_empty() || sys.mem.total > 0;
    let has_swap = sys.mem.swap_total > 0;
    let disk_lines = sys.disks.len().min(8);
    let mem_lines = if sys.mem.total > 0 { 1 } else { 0 };
    let swap_lines = if has_swap { 1 } else { 0 };
    let sys_height = if has_sys {
        (sys.cpus.len() + mem_lines + swap_lines + disk_lines + 2) as u16
    } else {
        0
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),          // IBD info (6 lines + 2 border)
            Constraint::Length(sys_height), // System stats (0 if unavailable)
            Constraint::Min(5),            // peers table
        ])
        .split(area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Initial Block Download ")
        .title_style(Style::default().fg(Color::Yellow).bold());
    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, rows[0]);

    if has_sys {
        draw_system_box(f, rows[1], sys);
    }

    draw_peers_table(f, rows[2], data, peer_scroll, false);
}

fn draw_system_box(f: &mut Frame, area: Rect, sys: &SystemStats) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" System ")
        .title_style(Style::default().fg(Color::Cyan).bold());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let available_w = inner.width as usize;

    // Label column: right-aligned, width = max(3, digits for highest core number)
    // "Mem" and "Swp" are 3 chars, core numbers could be wider for >=1000 cores
    let label_w = if sys.cpus.is_empty() {
        3
    } else {
        format!("{}", sys.cpus.len() - 1).len().max(3)
    };

    // Pre-compute info strings to find max width for right-aligned info after ]
    let cpu_infos: Vec<String> = sys.cpus.iter().map(|cpu| {
        let total = (cpu.nice_pct + cpu.user_pct + cpu.system_pct + cpu.iowait_pct).min(100.0);
        format!("{:.0}%", total)
    }).collect();
    let mem_info = if sys.mem.total > 0 {
        format!("{}/{}", format_bytes_short(sys.mem.used + sys.mem.buffers + sys.mem.cached), format_bytes_short(sys.mem.total))
    } else {
        String::new()
    };
    let swp_info = if sys.mem.swap_total > 0 {
        format!("{}/{}", format_bytes_short(sys.mem.swap_used), format_bytes_short(sys.mem.swap_total))
    } else {
        String::new()
    };
    let max_info = cpu_infos.iter().map(|s| s.len())
        .chain(std::iter::once(mem_info.len()))
        .chain(std::iter::once(swp_info.len()))
        .max()
        .unwrap_or(4);

    // Layout per line: " {label:>label_w}[{bar}] {info:>max_info}"
    // overhead = 1(pad) + label_w + 1([) + 1(]) + 1(space) + max_info
    let bar_w = available_w.saturating_sub(4 + label_w + max_info).max(10);

    let mut lines: Vec<Line> = Vec::new();

    // CPU bars: 1 per line
    for (i, cpu) in sys.cpus.iter().enumerate() {
        let total_pct = (cpu.nice_pct + cpu.user_pct + cpu.system_pct + cpu.iowait_pct).min(100.0);

        let nice_end = (cpu.nice_pct / 100.0 * bar_w as f32).round() as usize;
        let user_end = ((cpu.nice_pct + cpu.user_pct) / 100.0 * bar_w as f32).round() as usize;
        let sys_end = ((cpu.nice_pct + cpu.user_pct + cpu.system_pct) / 100.0 * bar_w as f32).round() as usize;
        let iow_end = (total_pct / 100.0 * bar_w as f32).round() as usize;

        let nice_c = nice_end.min(bar_w);
        let user_c = user_end.min(bar_w).saturating_sub(nice_c);
        let sys_c = sys_end.min(bar_w).saturating_sub(nice_c + user_c);
        let iow_c = iow_end.min(bar_w).saturating_sub(nice_c + user_c + sys_c);
        let empty_c = bar_w.saturating_sub(nice_c + user_c + sys_c + iow_c);

        let mut spans = vec![
            Span::styled(format!(" {:>w$}", i, w = label_w), Style::default().fg(Color::Cyan)),
            Span::styled("[", Style::default().fg(Color::DarkGray)),
        ];
        if nice_c > 0 { spans.push(Span::styled("|".repeat(nice_c), Style::default().fg(Color::Blue))); }
        if user_c > 0 { spans.push(Span::styled("|".repeat(user_c), Style::default().fg(Color::Green))); }
        if sys_c > 0 { spans.push(Span::styled("|".repeat(sys_c), Style::default().fg(Color::Red))); }
        if iow_c > 0 { spans.push(Span::styled("|".repeat(iow_c), Style::default().fg(Color::DarkGray))); }
        spans.push(Span::raw(" ".repeat(empty_c)));
        spans.push(Span::styled("]", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(format!(" {:>w$}", &cpu_infos[i], w = max_info), Style::default().fg(Color::White)));
        lines.push(Line::from(spans));
    }

    // Memory bar
    if sys.mem.total > 0 {
        let used_c = (sys.mem.used as f64 / sys.mem.total as f64 * bar_w as f64).round() as usize;
        let buf_c = (sys.mem.buffers as f64 / sys.mem.total as f64 * bar_w as f64).round() as usize;
        let cache_c = (sys.mem.cached as f64 / sys.mem.total as f64 * bar_w as f64).round() as usize;
        let filled = (used_c + buf_c + cache_c).min(bar_w);
        let empty = bar_w.saturating_sub(filled);

        let mut spans = vec![
            Span::styled(format!(" {:>w$}", "Mem", w = label_w), Style::default().fg(Color::Cyan)),
            Span::styled("[", Style::default().fg(Color::DarkGray)),
        ];
        if used_c > 0 { spans.push(Span::styled("|".repeat(used_c), Style::default().fg(Color::Green))); }
        if buf_c > 0 { spans.push(Span::styled("|".repeat(buf_c), Style::default().fg(Color::Blue))); }
        if cache_c > 0 { spans.push(Span::styled("|".repeat(cache_c), Style::default().fg(Color::Yellow))); }
        spans.push(Span::raw(" ".repeat(empty)));
        spans.push(Span::styled("]", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(format!(" {:>w$}", &mem_info, w = max_info), Style::default().fg(Color::White)));
        lines.push(Line::from(spans));
    }

    // Swap bar
    if sys.mem.swap_total > 0 {
        let used_c = (sys.mem.swap_used as f64 / sys.mem.swap_total as f64 * bar_w as f64).round() as usize;
        let empty = bar_w.saturating_sub(used_c);

        let mut spans = vec![
            Span::styled(format!(" {:>w$}", "Swp", w = label_w), Style::default().fg(Color::Cyan)),
            Span::styled("[", Style::default().fg(Color::DarkGray)),
        ];
        if used_c > 0 { spans.push(Span::styled("|".repeat(used_c), Style::default().fg(Color::Red))); }
        spans.push(Span::raw(" ".repeat(empty)));
        spans.push(Span::styled("]", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(format!(" {:>w$}", &swp_info, w = max_info), Style::default().fg(Color::White)));
        lines.push(Line::from(spans));
    }

    // Disk I/O per device
    let disk_name_w = sys.disks.iter().take(8).map(|d| d.name.len()).max().unwrap_or(3).max(label_w);
    for disk in sys.disks.iter().take(8) {
        let spans = vec![
            Span::styled(format!(" {:>w$}", disk.name, w = disk_name_w), Style::default().fg(Color::Cyan)),
            Span::styled("  R ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:>9}/s", format_bytes_short(disk.read_per_sec)), Style::default().fg(Color::Green)),
            Span::styled("  W ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:>9}/s", format_bytes_short(disk.write_per_sec)), Style::default().fg(Color::Yellow)),
        ];
        lines.push(Line::from(spans));
    }

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);
}

fn draw_body(f: &mut Frame, area: Rect, data: &NodeData, peer_scroll: u16, block_stats: &HashMap<u64, BlockStats>, selected_block: u16, block_scroll: u16, blocks_focused: bool, analytics: &AnalyticsData, system_stats: &SystemStats) {
    if let Some(ref err) = data.error {
        let err_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Error ")
            .style(Style::default().fg(Color::Red));
        let err_text = Paragraph::new(err.clone())
            .block(err_block)
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(Color::Red));
        f.render_widget(err_text, area);
        return;
    }

    if data.blockchain.initialblockdownload {
        draw_ibd_screen(f, area, data, peer_scroll, system_stats);
        return;
    }

    // Split body: top row (info cards) and bottom row (peers + blocks)
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(9), // top info cards
            Constraint::Min(8),    // bottom tables
        ])
        .split(area);

    // Top row: 4 cards
    let top_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(rows[0]);

    draw_blockchain_card(f, top_cols[0], data);
    draw_mempool_card(f, top_cols[1], data);
    draw_network_card(f, top_cols[2], data);
    draw_mining_card(f, top_cols[3], data);

    // Bottom: recent blocks, 24h analytics, peers
    let bottom_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(11), // 8 blocks + header + borders
            Constraint::Length(4),  // 24h analytics summary (header + data + borders)
            Constraint::Min(8),    // peers fills the rest
        ])
        .split(rows[1]);

    draw_blocks_table(f, bottom_rows[0], data, block_stats, selected_block, block_scroll, blocks_focused);
    draw_analytics_summary(f, bottom_rows[1], analytics);
    draw_peers_table(f, bottom_rows[2], data, peer_scroll, !blocks_focused);
}

fn draw_blockchain_card(f: &mut Frame, area: Rect, data: &NodeData) {
    let bc = &data.blockchain;
    let progress = bc.verificationprogress * 100.0;
    let synced = if progress >= 99.99 { "YES" } else { "syncing" };

    let lines = vec![
        Line::from(vec![
            Span::styled("Height:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format_number(bc.blocks),
                Style::default().fg(Color::Yellow).bold(),
            ),
        ]),
        Line::from(vec![
            Span::styled("Headers: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format_number(bc.headers), Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("Synced:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} ({:.2}%)", synced, progress),
                Style::default().fg(if progress >= 99.99 {
                    Color::Green
                } else {
                    Color::Yellow
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled("Diff:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{:.2e}", bc.difficulty),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("Disk:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format_bytes(bc.size_on_disk),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("Pruned:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if bc.pruned { "yes" } else { "no" }.to_string(),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("IBD:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if bc.initialblockdownload {
                    "yes"
                } else {
                    "no"
                }
                .to_string(),
                Style::default().fg(if bc.initialblockdownload {
                    Color::Yellow
                } else {
                    Color::Green
                }),
            ),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Blockchain ")
        .title_style(Style::default().fg(Color::Cyan).bold());

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

fn draw_mempool_card(f: &mut Frame, area: Rect, data: &NodeData) {
    let mp = &data.mempool;

    let lines = vec![
        Line::from(vec![
            Span::styled("TXs:      ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format_number(mp.size),
                Style::default().fg(Color::Yellow).bold(),
            ),
        ]),
        Line::from(vec![
            Span::styled("Size:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format_bytes(mp.bytes),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("Memory:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(
                    "{} / {}",
                    format_bytes(mp.usage),
                    format_bytes(mp.maxmempool)
                ),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("Fees:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{:.8} BTC", mp.total_fee),
                Style::default().fg(Color::Green),
            ),
        ]),
        Line::from(vec![
            Span::styled("Min fee:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{:.8} BTC/kvB", mp.mempoolminfee),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("Relay fee:", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" {:.8} BTC/kvB", mp.minrelaytxfee),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("Loaded:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if mp.loaded { "yes" } else { "no" }.to_string(),
                Style::default().fg(if mp.loaded {
                    Color::Green
                } else {
                    Color::Yellow
                }),
            ),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Mempool ")
        .title_style(Style::default().fg(Color::Magenta).bold());

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

fn draw_network_card(f: &mut Frame, area: Rect, data: &NodeData) {
    let net = &data.network;
    let nt = &data.net_totals;

    let lines = vec![
        Line::from(vec![
            Span::styled("Conns:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(
                    "{} (in: {} / out: {})",
                    net.connections, net.connections_in, net.connections_out
                ),
                Style::default().fg(Color::Yellow).bold(),
            ),
        ]),
        Line::from(vec![
            Span::styled("Protocol: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", net.protocolversion),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("Recv:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format_bytes(nt.totalbytesrecv),
                Style::default().fg(Color::Green),
            ),
        ]),
        Line::from(vec![
            Span::styled("Sent:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format_bytes(nt.totalbytessent),
                Style::default().fg(Color::Red),
            ),
        ]),
        Line::from(vec![
            Span::styled("Relay fee:", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" {:.8} BTC/kvB", net.relayfee),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("Incr fee: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{:.8} BTC/kvB", net.incrementalfee),
                Style::default().fg(Color::White),
            ),
        ]),
    ];

    // Append local addresses if any
    let mut all_lines = lines;
    if !net.localaddresses.is_empty() {
        let addrs: Vec<String> = net
            .localaddresses
            .iter()
            .map(|a| format!("{}:{}", a.address, a.port))
            .collect();
        all_lines.push(Line::from(vec![
            Span::styled("Local:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(addrs.join(", "), Style::default().fg(Color::White)),
        ]));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Network ")
        .title_style(Style::default().fg(Color::Green).bold());

    let paragraph = Paragraph::new(all_lines).block(block);
    f.render_widget(paragraph, area);
}

fn draw_mining_card(f: &mut Frame, area: Rect, data: &NodeData) {
    let mi = &data.mining;

    let hashrate = format_hashrate(mi.networkhashps);

    let warnings_text = data.blockchain.warnings.as_str();

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Hashrate: ", Style::default().fg(Color::DarkGray)),
            Span::styled(hashrate, Style::default().fg(Color::Yellow).bold()),
        ]),
        Line::from(vec![
            Span::styled("Pooled TX:", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" {}", mi.pooledtx),
                Style::default().fg(Color::White),
            ),
        ]),
    ];

    if !warnings_text.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Warnings:",
            Style::default().fg(Color::Red).bold(),
        )));
        // Wrap warning text into multiple lines if needed
        for chunk in warnings_text.as_bytes().chunks(area.width.saturating_sub(4) as usize) {
            if let Ok(s) = std::str::from_utf8(chunk) {
                lines.push(Line::from(Span::styled(
                    s.to_string(),
                    Style::default().fg(Color::Red),
                )));
            }
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Mining / Warnings ")
        .title_style(Style::default().fg(Color::Yellow).bold());

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

fn draw_analytics_summary(f: &mut Frame, area: Rect, analytics: &AnalyticsData) {
    let now = chrono::Utc::now().timestamp() as u64;
    let cutoff = now.saturating_sub(86400);
    let agg = aggregate_period(&analytics.stats, cutoff);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Analytics ")
        .style(Style::default().fg(Color::Cyan));
    if agg.blocks > 0 {
        let rows = vec![analytics_data_row("24h", &agg)];
        let table = Table::new(rows, analytics_widths())
            .header(analytics_header_row())
            .block(block);
        f.render_widget(table, area);
    } else {
        let paragraph = Paragraph::new(Line::from(Span::styled(
            "  Waiting for block analysis data...",
            Style::default().fg(Color::DarkGray),
        )))
        .block(block);
        f.render_widget(paragraph, area);
    }
}

fn draw_peers_table(f: &mut Frame, area: Rect, data: &NodeData, scroll: u16, focused: bool) {
    let header = Row::new(vec![
        "ID", "Address", "Client", "Type", "TX", "Dir", "Height",
        "Ping", "Conn", "Sent", "Recv",
    ])
        .style(Style::default().fg(Color::Cyan).bold())
        .bottom_margin(0);

    let now = data.fetched_at;

    let rows: Vec<Row> = data
        .peers
        .iter()
        .map(|p| {
            let dir = if p.inbound { "in" } else { "out" };
            let ping = p
                .pingtime
                .map(|t| format!("{:.0}ms", t * 1000.0))
                .unwrap_or_else(|| "-".to_string());

            let client = p.subver.trim_matches('/').to_string();

            let uptime = if p.conntime > 0 && now > p.conntime {
                format_duration(now - p.conntime)
            } else {
                "-".to_string()
            };

            let relay = if p.relaytxes { "yes" } else { "no" };

            Row::new(vec![
                p.id.to_string(),
                p.addr.clone(),
                client,
                p.connection_type.clone(),
                relay.to_string(),
                dir.to_string(),
                p.synced_blocks.to_string(),
                ping,
                uptime,
                format_bytes_short(p.bytessent),
                format_bytes_short(p.bytesrecv),
            ])
            .style(Style::default().fg(if p.inbound {
                Color::White
            } else {
                Color::Gray
            }))
        })
        .collect();

    let widths = [
        Constraint::Length(4),
        Constraint::Min(18),
        Constraint::Min(30),
        Constraint::Length(19),
        Constraint::Length(3),
        Constraint::Length(4),
        Constraint::Length(8),
        Constraint::Length(7),
        Constraint::Length(12),
        Constraint::Length(8),
        Constraint::Length(8),
    ];

    let border_color = if focused { Color::Yellow } else { Color::default() };
    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color))
                .title(format!(
                    " Peers ({}) | known: {} ",
                    data.peers.len(),
                    format_number(data.known_peers)
                ))
                .title_style(Style::default().fg(Color::Cyan).bold()),
        )
        .row_highlight_style(Style::default().bg(Color::DarkGray));

    // Manual scroll via offset
    let mut state = TableState::default().with_offset(scroll as usize);
    f.render_stateful_widget(table, area, &mut state);
}

fn draw_blocks_table(f: &mut Frame, area: Rect, data: &NodeData, block_stats: &HashMap<u64, BlockStats>, selected_block: u16, block_scroll: u16, focused: bool) {
    let header = Row::new(vec![
        Cell::from(" "),
        Cell::from("Height"),
        Cell::from("Time"),
        Cell::from("TXs"),
        Cell::from("Size"),
        Cell::from("Weight"),
        Cell::from("Age"),
        Cell::from("BIP110"),
        Cell::from("BTC Out"),
        Cell::from("Fees"),
        Cell::from("Fin%"),
        Cell::from(Line::from("!110").alignment(Alignment::Right)),
        Cell::from("%"),
    ])
        .style(Style::default().fg(Color::Cyan).bold())
        .bottom_margin(0);

    let now = chrono::Utc::now().timestamp() as u64;
    let scroll = block_scroll as usize;
    let visible_blocks: Vec<(usize, &BlockInfo)> = data
        .recent_blocks
        .iter()
        .enumerate()
        .skip(scroll)
        .take(8)
        .collect();

    let rows: Vec<Row> = visible_blocks
        .iter()
        .map(|&(i, b)| {
            let age = if b.time > 0 && now > b.time {
                format_duration(now - b.time)
            } else {
                "-".to_string()
            };
            let bip110 = if b.version >= 0x20000000 && b.version & (1 << 4) != 0 {
                "yes"
            } else {
                "no"
            };
            let bip110_color = if bip110 == "yes" { Color::Green } else { Color::DarkGray };
            let marker = if focused && i == selected_block as usize { ">" } else { " " };

            let (btc_out, fees, financial, fin_color, viol_count, viol_pct_str, viol_color) = if let Some(s) = block_stats.get(&b.height) {
                let user_tx = s.tx_count.saturating_sub(1);
                let pct = if user_tx > 0 {
                    (s.financial_count as f64 / user_tx as f64) * 100.0
                } else {
                    100.0
                };
                let color = if pct >= 90.0 { Color::Green } else if pct >= 70.0 { Color::Yellow } else { Color::Red };
                let viol_pct = if user_tx > 0 { s.bip110_violating_txs as f64 / user_tx as f64 * 100.0 } else { 0.0 };
                let vc = if s.bip110_violating_txs == 0 { Color::Green } else if viol_pct <= 1.0 { Color::Yellow } else { Color::Red };
                let count_str = format!("{}", s.bip110_violating_txs);
                let pct_str = if s.bip110_violating_txs > 0 { format!("{:.1}%", viol_pct) } else { String::new() };
                (format_btc(s.total_out), format_btc_fees(s.total_fee), format!("{:.0}%", pct), color, count_str, pct_str, vc)
            } else {
                ("-".to_string(), "-".to_string(), "-".to_string(), Color::DarkGray, "-".to_string(), String::new(), Color::DarkGray)
            };

            let timestamp = if b.time > 0 {
                chrono::DateTime::from_timestamp(b.time as i64, 0)
                    .map(|dt| dt.format("%m-%d %H:%M").to_string())
                    .unwrap_or("-".to_string())
            } else {
                "-".to_string()
            };

            Row::new(vec![
                Cell::from(Span::styled(marker, Style::default().fg(Color::Yellow))),
                Cell::from(format_number(b.height)),
                Cell::from(timestamp),
                Cell::from(format_number(b.tx_count as u64)),
                Cell::from(format_bytes_short(b.size)),
                Cell::from(format!("{:.1} kvWU", b.weight as f64 / 1000.0)),
                Cell::from(age),
                Cell::from(Span::styled(bip110.to_string(), Style::default().fg(bip110_color))),
                Cell::from(btc_out),
                Cell::from(fees),
                Cell::from(Span::styled(financial, Style::default().fg(fin_color))),
                Cell::from(Line::from(Span::styled(viol_count, Style::default().fg(viol_color))).alignment(Alignment::Right)),
                Cell::from(Span::styled(viol_pct_str, Style::default().fg(viol_color))),
            ])
        })
        .collect();

    let widths = vec![
        Constraint::Length(2),  // marker
        Constraint::Length(10), // Height
        Constraint::Length(12), // Time
        Constraint::Length(7),  // TXs
        Constraint::Length(10), // Size
        Constraint::Length(12), // Weight
        Constraint::Length(12), // Age
        Constraint::Length(7),  // BIP110
        Constraint::Length(12), // BTC Out
        Constraint::Length(12), // Fees
        Constraint::Length(5),  // Fin%
        Constraint::Length(5),  // !110
        Constraint::Min(5),    // !110%
    ];

    let total = data.recent_blocks.len();
    let title = format!(" Recent Blocks ({}-{}/{}) [Enter: detail] ", scroll + 1, (scroll + 8).min(total), total);
    let border_color = if focused { Color::Yellow } else { Color::default() };

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color))
                .title(title)
                .title_style(Style::default().fg(Color::Yellow).bold()),
        );

    f.render_widget(table, area);
}

fn draw_known_peers(f: &mut Frame, area: Rect, data: &NodeData, scroll: u16) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10), // time buckets table
            Constraint::Min(8),    // services table
        ])
        .split(area);

    draw_known_peers_time(f, layout[0], data);
    draw_known_peers_services(f, layout[1], data, scroll);
}

fn net_color(net: &str) -> Color {
    match net {
        "ipv4" => Color::Green,
        "ipv6" => Color::Blue,
        "onion" => Color::Magenta,
        "i2p" => Color::Yellow,
        "cjdns" => Color::Cyan,
        _ => Color::Gray,
    }
}

fn draw_known_peers_time(f: &mut Frame, area: Rect, data: &NodeData) {
    let now = data.fetched_at;

    let bucket_labels = ["1d", "2d", "3d", "4d", "5d", "6d", "7d", "7-14d", "14-30d", "30d+"];
    let day: u64 = 86400;
    let bucket_thresholds: [u64; 10] = [
        day, 2*day, 3*day, 4*day, 5*day, 6*day, 7*day, 14*day, 30*day, u64::MAX,
    ];

    let mut network_buckets: BTreeMap<String, [u64; 10]> = BTreeMap::new();

    for addr in &data.known_addresses {
        let net = if addr.network.is_empty() {
            "unknown".to_string()
        } else {
            addr.network.clone()
        };
        let age = now.saturating_sub(addr.time);
        let bucket_idx = bucket_thresholds
            .iter()
            .position(|&t| age < t)
            .unwrap_or(9);

        let buckets = network_buckets.entry(net).or_insert([0; 10]);
        buckets[bucket_idx] += 1;
    }

    let mut totals = [0u64; 10];
    for buckets in network_buckets.values() {
        for (i, &count) in buckets.iter().enumerate() {
            totals[i] += count;
        }
    }

    let mut header_cells = vec!["Network".to_string()];
    header_cells.extend(bucket_labels.iter().map(|s| s.to_string()));
    header_cells.push("Total".to_string());

    let header = Row::new(header_cells)
        .style(Style::default().fg(Color::Cyan).bold())
        .bottom_margin(0);

    let mut rows: Vec<Row> = network_buckets
        .iter()
        .map(|(net, buckets)| {
            let total: u64 = buckets.iter().sum();
            let mut cells = vec![net.clone()];
            for &count in buckets {
                cells.push(format_number(count));
            }
            cells.push(format_number(total));
            Row::new(cells).style(Style::default().fg(net_color(net)))
        })
        .collect();

    let grand_total: u64 = totals.iter().sum();
    let mut total_cells = vec!["TOTAL".to_string()];
    for &count in &totals {
        total_cells.push(format_number(count));
    }
    total_cells.push(format_number(grand_total));
    rows.push(
        Row::new(total_cells)
            .style(Style::default().fg(Color::White).bold()),
    );

    let widths = [
        Constraint::Length(10),
        Constraint::Length(7),
        Constraint::Length(7),
        Constraint::Length(7),
        Constraint::Length(7),
        Constraint::Length(7),
        Constraint::Length(7),
        Constraint::Length(7),
        Constraint::Length(7),
        Constraint::Length(7),
        Constraint::Length(7),
        Constraint::Length(10),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(format!(
                    " Known Addresses ({}) by Last Seen ",
                    format_number(data.known_peers)
                ))
                .title_style(Style::default().fg(Color::Cyan).bold()),
        );

    f.render_widget(table, area);
}

fn format_count_pct(count: u64, total: u64) -> String {
    if total == 0 {
        return "0".to_string();
    }
    let pct = (count as f64 / total as f64) * 100.0;
    format!("{} ({:.0}%)", format_number(count), pct)
}

fn service_bit_name(bit: u8) -> &'static str {
    match bit {
        0 => "NODE_NETWORK",
        1 => "NODE_GETUTXO",
        2 => "NODE_BLOOM",
        3 => "NODE_WITNESS",
        6 => "NODE_COMPACT_FILTERS",
        10 => "NODE_NETWORK_LIMITED",
        24 => "NODE_P2P_V2",
        27 => "NODE_REDUCED_DATA",
        _ => "",
    }
}

fn service_bit_desc(bit: u8) -> &'static str {
    match bit {
        0 => "Serves all blocks since genesis",
        1 => "Serves UTXO queries (BIP64)",
        2 => "SPV bloom filter queries (BIP111)",
        3 => "Understands SegWit (BIP144)",
        6 => "Serves BIP157 compact block filters for light clients",
        10 => "Pruned node, serves last 288 blocks only",
        24 => "Encrypted P2P via v2 transport (BIP324)",
        27 => "Enforces BIP-110 ReducedData rules",
        _ => "",
    }
}

fn draw_known_peers_services(f: &mut Frame, area: Rect, data: &NodeData, scroll: u16) {
    // Parse node's own service flags from hex string
    let local_services: u64 = u64::from_str_radix(
        data.network.localservices.trim_start_matches("0x"),
        16,
    )
    .unwrap_or(0);

    // Discover all service bits present in the dataset
    let mut all_bits_mask: u64 = 0;
    for addr in &data.known_addresses {
        all_bits_mask |= addr.services;
    }

    let mut active_bits: Vec<u8> = Vec::new();
    for bit in 0..64u8 {
        if all_bits_mask & (1u64 << bit) != 0 {
            active_bits.push(bit);
        }
    }

    // Count per network per bit
    // networks sorted, plus totals per network
    let mut by_network: BTreeMap<String, (u64, Vec<u64>)> = BTreeMap::new();

    for addr in &data.known_addresses {
        let net = if addr.network.is_empty() {
            "unknown".to_string()
        } else {
            addr.network.clone()
        };
        let entry = by_network
            .entry(net)
            .or_insert_with(|| (0, vec![0u64; active_bits.len()]));
        entry.0 += 1;
        for (i, &bit) in active_bits.iter().enumerate() {
            if addr.services & (1u64 << bit) != 0 {
                entry.1[i] += 1;
            }
        }
    }

    let networks: Vec<String> = by_network.keys().cloned().collect();

    // Header: Service | network1 | network2 | ... | TOTAL | Description
    let mut header_cells = vec!["Service".to_string()];
    for net in &networks {
        header_cells.push(net.clone());
    }
    header_cells.push("TOTAL".to_string());
    header_cells.push("Description".to_string());

    let header = Row::new(header_cells)
        .style(Style::default().fg(Color::Cyan).bold())
        .bottom_margin(0);

    // Aggregate totals across all networks
    let grand_total: u64 = by_network.values().map(|(t, _)| t).sum();

    // Compute total per bit for sorting
    let mut bit_totals: Vec<(usize, u64)> = active_bits
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let total: u64 = networks
                .iter()
                .map(|net| by_network[net].1[i])
                .sum();
            (i, total)
        })
        .collect();
    bit_totals.sort_by(|a, b| b.1.cmp(&a.1));

    // Rows: one per service bit, sorted by total descending
    // Bits the node itself has are marked with *
    let mut rows: Vec<Row> = bit_totals
        .iter()
        .map(|&(i, bit_total)| {
            let bit = active_bits[i];
            let is_local = local_services & (1u64 << bit) != 0;
            let known_name = service_bit_name(bit);
            let name = if is_local {
                if known_name.is_empty() {
                    format!("* bit{}", bit)
                } else {
                    format!("* {}", known_name)
                }
            } else if known_name.is_empty() {
                format!("  bit{}", bit)
            } else {
                format!("  {}", known_name)
            };
            let mut cells = vec![name];

            for net in &networks {
                let (net_total, bit_counts) = &by_network[net];
                let count = bit_counts[i];
                cells.push(format_count_pct(count, *net_total));
            }

            cells.push(format_count_pct(bit_total, grand_total));
            cells.push(service_bit_desc(bit).to_string());
            let color = if is_local { Color::Green } else { Color::White };
            Row::new(cells).style(Style::default().fg(color))
        })
        .collect();

    // Widths: service name + networks + total + description
    let mut widths = vec![Constraint::Length(24)];
    for _ in &networks {
        widths.push(Constraint::Length(14));
    }
    widths.push(Constraint::Length(14));
    widths.push(Constraint::Min(24));

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Services by Network (* = this node) [↑/↓ scroll] ")
                .title_style(Style::default().fg(Color::Yellow).bold()),
        );

    let mut state = TableState::default().with_offset(scroll as usize);
    f.render_stateful_widget(table, area, &mut state);
}

fn draw_signaling(f: &mut Frame, area: Rect, data: &NodeData, selected_bit: u8) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(6),    // version bits from recent blocks
            Constraint::Length(14), // softforks table
        ])
        .split(area);

    draw_version_bits(f, layout[0], data, selected_bit);
    draw_softforks(f, layout[1], data);
}

fn known_buried_softforks() -> BTreeMap<String, crate::rpc::SoftFork> {
    use crate::rpc::SoftFork;
    let mut m = BTreeMap::new();
    let buried = |height: i64| SoftFork {
        fork_type: "buried".to_string(),
        active: true,
        height: Some(height),
        bip9: None,
    };
    m.insert("bip34".to_string(), buried(227931));
    m.insert("bip66".to_string(), buried(363725));
    m.insert("bip65".to_string(), buried(388381));
    m.insert("csv".to_string(), buried(419328));
    m.insert("segwit".to_string(), buried(481824));
    m.insert("taproot".to_string(), buried(709632));
    m
}

fn draw_softforks(f: &mut Frame, area: Rect, data: &NodeData) {
    let header = Row::new(vec![
        "Name", "Type", "Active", "Height", "Status", "Bit", "Progress",
    ])
        .style(Style::default().fg(Color::Cyan).bold())
        .bottom_margin(0);

    // Merge node softforks with known buried defaults
    let mut merged = known_buried_softforks();
    for (name, fork) in &data.softforks {
        merged.insert(name.clone(), fork.clone());
    }

    // Sort: BIP9 (non-buried) first, then buried by height descending (newest first)
    let mut sorted: Vec<(&String, &crate::rpc::SoftFork)> = merged.iter().collect();
    sorted.sort_by(|(_, a), (_, b)| {
        let a_buried = a.fork_type == "buried";
        let b_buried = b.fork_type == "buried";
        match (a_buried, b_buried) {
            (false, true) => std::cmp::Ordering::Less,
            (true, false) => std::cmp::Ordering::Greater,
            _ => {
                // Within same type, sort by height descending (newest first)
                let ah = a.height.unwrap_or(i64::MAX);
                let bh = b.height.unwrap_or(i64::MAX);
                bh.cmp(&ah)
            }
        }
    });

    let rows: Vec<Row> = sorted
        .iter()
        .map(|(name, fork)| {
            let is_buried = fork.fork_type == "buried";
            let active_str = if fork.active { "yes" } else { "no" };

            let height_str = fork
                .height
                .map(|h| format_number(h as u64))
                .unwrap_or_else(|| "-".to_string());

            let (status, bit, progress) = if let Some(ref bip9) = fork.bip9 {
                let bit_str = bip9
                    .bit
                    .map(|b| b.to_string())
                    .unwrap_or_else(|| "-".to_string());

                let progress = if let Some(ref stats) = bip9.statistics {
                    format!(
                        "{}/{} ({:.1}%)",
                        format_number(stats.count),
                        format_number(stats.period),
                        if stats.period > 0 {
                            (stats.count as f64 / stats.period as f64) * 100.0
                        } else {
                            0.0
                        }
                    )
                } else {
                    "-".to_string()
                };

                (bip9.status.clone(), bit_str, progress)
            } else {
                (
                    if is_buried { "buried".to_string() } else { "-".to_string() },
                    "-".to_string(),
                    "-".to_string(),
                )
            };

            let color = if is_buried {
                Color::DarkGray
            } else if fork.active {
                Color::Green
            } else {
                match status.as_str() {
                    "started" => Color::Yellow,
                    "locked_in" => Color::Cyan,
                    "defined" => Color::DarkGray,
                    "failed" => Color::Red,
                    _ => Color::White,
                }
            };

            Row::new(vec![
                (*name).clone(),
                fork.fork_type.clone(),
                active_str.to_string(),
                height_str,
                status.clone(),
                bit,
                progress,
            ])
            .style(Style::default().fg(color))
        })
        .collect();

    let widths = [
        Constraint::Length(16),
        Constraint::Length(8),
        Constraint::Length(7),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(4),
        Constraint::Min(20),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(format!(
                    " Softforks ({}) ",
                    merged.len()
                ))
                .title_style(Style::default().fg(Color::Cyan).bold()),
        );

    f.render_widget(table, area);
}

fn draw_version_bits(f: &mut Frame, area: Rect, data: &NodeData, selected_bit: u8) {
    if data.recent_block_versions.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Version Bit Signaling (last 144 blocks) ")
            .title_style(Style::default().fg(Color::Yellow).bold());
        let p = Paragraph::new("No block data").block(block);
        f.render_widget(p, area);
        return;
    }

    // Find all version bits set across recent blocks (bits 0-28 per BIP9)
    let mut bit_counts: BTreeMap<u8, u64> = BTreeMap::new();
    let total_blocks = data.recent_block_versions.len() as u64;

    for &(_height, version) in &data.recent_block_versions {
        // BIP9 version bits are bits 0-28 when top 3 bits = 001 (version >= 0x20000000)
        if version >= 0x20000000 {
            for bit in 0..29u8 {
                if version & (1i64 << bit) != 0 {
                    *bit_counts.entry(bit).or_insert(0) += 1;
                }
            }
        }
    }

    // Build bit name and description lookups
    let mut bit_names: BTreeMap<u8, String> = BTreeMap::new();
    let mut bit_descs: BTreeMap<u8, String> = BTreeMap::new();

    // Known BIP9 version bit assignments
    bit_names.insert(0, "csv".to_string());
    bit_descs.insert(0, "Relative lock-time (BIP68/112/113)".to_string());
    bit_names.insert(1, "segwit".to_string());
    bit_descs.insert(1, "Segregated Witness (BIP141/143/147)".to_string());
    bit_names.insert(2, "taproot".to_string());
    bit_descs.insert(2, "Taproot/Schnorr (BIP340/341/342)".to_string());
    bit_names.insert(4, "reduced_data".to_string());
    bit_descs.insert(4, "Reduced Data Temporary Softfork (BIP110)".to_string());

    // Unassigned signaling bits
    for bit in [3u8, 5, 6, 7, 8, 9, 10, 11, 12] {
        bit_descs.entry(bit).or_insert_with(|| "Unassigned signaling bit".to_string());
    }

    // BIP320: bits 13-28 reserved for miner nonce rolling
    for bit in 13..=28u8 {
        bit_descs.insert(bit, "BIP320 nonce rolling (ASICBoost)".to_string());
    }

    // Override names with live softfork data from the node
    for (name, fork) in &data.softforks {
        if let Some(ref bip9) = fork.bip9 {
            if let Some(bit) = bip9.bit {
                bit_names.insert(bit, name.clone());
            }
        }
    }

    let header = Row::new(vec!["Bit", "Name", "Signaling", "Pct", "Description"])
        .style(Style::default().fg(Color::Cyan).bold())
        .bottom_margin(0);

    // Show all 29 bits ordered by bit number
    let rows: Vec<Row> = (0..29u8)
        .map(|bit| {
            let count = bit_counts.get(&bit).copied().unwrap_or(0);
            let is_bip320 = bit >= 13 && bit <= 28;
            let name = bit_names
                .get(&bit)
                .cloned()
                .unwrap_or_default();
            let desc = bit_descs
                .get(&bit)
                .cloned()
                .unwrap_or_default();
            let pct = (count as f64 / total_blocks as f64) * 100.0;
            let color = if is_bip320 {
                Color::DarkGray
            } else if pct >= 95.0 {
                Color::Green
            } else if pct >= 50.0 {
                Color::Yellow
            } else {
                Color::White
            };

            let marker = if bit == selected_bit { ">" } else { " " };

            Row::new(vec![
                format!("{} {:>2}", marker, bit),
                name,
                format!("{}/{}", format_number(count), format_number(total_blocks)),
                format!("{:.1}%", pct),
                desc,
            ])
            .style(Style::default().fg(color))
        })
        .collect();

    let widths = [
        Constraint::Length(5),
        Constraint::Length(22),
        Constraint::Length(12),
        Constraint::Length(8),
        Constraint::Min(20),
    ];

    // Count blocks with BIP9 version bit prefix
    let bip9_blocks = data
        .recent_block_versions
        .iter()
        .filter(|&&(_, v)| v >= 0x20000000)
        .count() as u64;

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(format!(
                    " Version Bit Signaling (last {} blocks, {} BIP9-versioned) [j/k select, Enter: details] ",
                    total_blocks, bip9_blocks
                ))
                .title_style(Style::default().fg(Color::Yellow).bold()),
        )
        .row_highlight_style(Style::default().bg(Color::Rgb(40, 40, 60)));

    let mut state = TableState::default().with_selected(selected_bit as usize);
    f.render_stateful_widget(table, area, &mut state);
}

fn bit_detail(bit: u8) -> (&'static str, &'static str) {
    match bit {
        0 => ("BIP9 Bit 0: csv (BIP68/112/113)", "\
Relative lock-time using sequence numbers. Three BIPs activated together:\n\
\n\
BIP68 - Redefines nSequence field to encode relative lock-time,\n\
  allowing transactions to be time-locked relative to the block\n\
  that confirmed the parent output (not an absolute block height).\n\
\n\
BIP112 - Adds OP_CHECKSEQUENCEVERIFY opcode so scripts can enforce\n\
  that an output cannot be spent until a relative time has passed.\n\
  Essential for payment channels and Lightning Network HTLCs.\n\
\n\
BIP113 - Uses median-time-past (MTP) instead of block timestamp\n\
  for time-based lock evaluation, preventing miner manipulation.\n\
\n\
Activated at block 419,328 (July 2016). Threshold: 95%."),

        1 => ("BIP9 Bit 1: segwit (BIP141/143/147)", "\
Segregated Witness - the largest protocol upgrade to Bitcoin.\n\
\n\
BIP141 - Moves signature data (witness) outside the base block,\n\
  creating a new weight-based block limit of 4M weight units.\n\
  Fixes transaction malleability by excluding witness from txid.\n\
\n\
BIP143 - New signature hash algorithm for SegWit inputs that\n\
  includes the input value, preventing signing-time attacks\n\
  and enabling efficient hardware wallet verification.\n\
\n\
BIP147 - Fixes a dummy stack element malleability in CHECKMULTISIG\n\
  by requiring it to be exactly OP_0 (null dummy).\n\
\n\
Activated at block 481,824 (August 2017). Threshold: 95%.\n\
Enabled Lightning Network, reduced fees, fixed tx malleability."),

        2 => ("BIP9 Bit 2: taproot (BIP340/341/342)", "\
Taproot - Schnorr signatures and advanced scripting.\n\
\n\
BIP340 - Schnorr signature scheme: more efficient than ECDSA,\n\
  enables key and signature aggregation (MuSig2), and provides\n\
  batch verification for faster block validation.\n\
\n\
BIP341 - Taproot output structure using a tweaked public key.\n\
  The common spend path (key path) looks like a regular payment,\n\
  improving privacy. Complex scripts are hidden in a Merkle tree\n\
  and only revealed if the key path isn't used.\n\
\n\
BIP342 - Tapscript: updated Script rules for Taproot, adds\n\
  OP_CHECKSIGADD for flexible multisig, removes OP_CHECKMULTISIG,\n\
  and uses Schnorr-only signature checking in scripts.\n\
\n\
Activated at block 709,632 (November 2021). Speedy Trial method."),

        4 => ("BIP9 Bit 4: reduced_data (BIP110)", "\
Reduced Data Temporary Soft Fork - limits certain transaction\n\
data to reduce node resource consumption.\n\
\n\
BIP110 proposes temporary restrictions on data-carrying\n\
transactions (like OP_RETURN outputs and witness data) to\n\
reduce blockchain bloat and state growth.\n\
\n\
This is specific to Bitcoin Knots and is not activated on\n\
Bitcoin Core. It aims to address concerns about non-financial\n\
data stored on-chain consuming node resources.\n\
\n\
Status: Defined/proposed. Not yet widely signaled."),

        3 | 5..=12 => ("Unassigned BIP9 Signaling Bit", "\
This bit is currently unassigned and available for future\n\
soft fork proposals using the BIP9 signaling mechanism.\n\
\n\
BIP9 reserves bits 0-28 for miners to signal readiness for\n\
consensus rule changes. Each proposed soft fork is assigned\n\
a specific bit during its signaling period. Once a deployment\n\
succeeds or times out, the bit is freed for reuse.\n\
\n\
If blocks are signaling this bit, it may indicate:\n\
- A new proposal not yet recognized by this software\n\
- Testing or experimental signaling\n\
- Random noise (unlikely for low bits)"),

        13..=28 => ("BIP320: Version Rolling for ASICBoost", "\
This bit is reserved for miner nonce rolling under BIP320.\n\
\n\
Modern ASIC miners use a technique called ASICBoost that\n\
manipulates the block header to gain mining efficiency.\n\
BIP320 designates bits 13-28 of the version field as\n\
general-purpose bits that miners can freely toggle as\n\
additional nonce space.\n\
\n\
The ~50% signaling rate on each of these bits is expected:\n\
miners cycle through these bits randomly while hashing,\n\
so each bit is set roughly half the time by chance.\n\
\n\
These bits are NOT used for soft fork signaling and should\n\
be ignored when evaluating protocol upgrade readiness."),

        _ => ("Unknown Bit", "No information available for this bit."),
    }
}

fn draw_bit_modal(f: &mut Frame, area: Rect, selected_bit: u8, data: &NodeData) {
    // Modal size: 60% width, 60% height, centered
    let modal_width = (area.width as f32 * 0.65) as u16;
    let modal_height = (area.height as f32 * 0.6) as u16;
    let x = (area.width.saturating_sub(modal_width)) / 2;
    let y = (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(x, y, modal_width, modal_height);

    // Dim the background
    let dim = Block::default().style(Style::default().bg(Color::Black));
    f.render_widget(Clear, modal_area);
    f.render_widget(dim, area);

    let (title, detail) = bit_detail(selected_bit);

    // Get signaling stats for this bit
    let total_blocks = data.recent_block_versions.len() as u64;
    let count = data
        .recent_block_versions
        .iter()
        .filter(|&&(_, v)| v >= 0x20000000 && v & (1i64 << selected_bit) != 0)
        .count() as u64;
    let pct = if total_blocks > 0 {
        (count as f64 / total_blocks as f64) * 100.0
    } else {
        0.0
    };

    let stats_line = format!(
        "Signaling: {}/{} blocks ({:.1}%)\n",
        format_number(count),
        format_number(total_blocks),
        pct
    );

    let mut text = vec![
        Line::from(Span::styled(
            stats_line,
            Style::default().fg(Color::Yellow).bold(),
        )),
        Line::from(""),
    ];

    for line in detail.lines() {
        text.push(Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(Color::White),
        )));
    }

    text.push(Line::from(""));
    text.push(Line::from(Span::styled(
        "Press Esc or Enter to close",
        Style::default().fg(Color::DarkGray),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .title(format!(" {} ", title))
        .title_style(Style::default().fg(Color::Cyan).bold())
        .style(Style::default().bg(Color::Black));

    let paragraph = Paragraph::new(text)
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(Clear, modal_area);
    f.render_widget(paragraph, modal_area);
}

fn draw_block_modal(f: &mut Frame, area: Rect, block: &BlockInfo, stats: &BlockStats) {
    let modal_width = (area.width as f32 * 0.65) as u16;
    let modal_height = 43u16.min(area.height - 4);
    let x = (area.width.saturating_sub(modal_width)) / 2;
    let y = (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(x, y, modal_width, modal_height);

    let dim = Block::default().style(Style::default().bg(Color::Black));
    f.render_widget(Clear, modal_area);
    f.render_widget(dim, area);

    let user_tx = stats.tx_count.saturating_sub(1); // exclude coinbase
    let data_count = user_tx.saturating_sub(stats.financial_count);
    let data_vsize = stats.total_vsize.saturating_sub(stats.financial_vsize);
    let pct = |count: usize| -> f64 {
        if user_tx > 0 { (count as f64 / user_tx as f64) * 100.0 } else { 0.0 }
    };
    let fin_pct = pct(stats.financial_count);
    let data_pct = pct(data_count);
    let taproot_spend_pct = pct(stats.taproot_spend_count);
    let taproot_output_pct = pct(stats.taproot_output_count);

    // Yellow if non-zero, grey if zero
    let proto_color = |count: usize| -> Color {
        if count > 0 { Color::Yellow } else { Color::DarkGray }
    };
    // Red if non-zero (violation), green if zero (clean)
    let viol_color = |count: usize| -> Color {
        if count > 0 { Color::Red } else { Color::Green }
    };

    // Protocol rows: (label, count, vsize, description)
    let protocols: Vec<(&str, usize, u64, &str)> = vec![
        ("Runes",          stats.rune_count,          stats.rune_vsize,          "fungible tokens via OP_RETURN"),
        ("BRC-20",         stats.brc20_count,         stats.brc20_vsize,         "token standard via ordinals"),
        ("Inscriptions",   stats.inscription_count,   stats.inscription_vsize,   "ordinals data (images, text, etc.)"),
        ("OPNET",          stats.opnet_count,          stats.opnet_vsize,         "smart contracts via tapscript"),
        ("Stamps",         stats.stamp_count,          stats.stamp_vsize,         "SRC-20 tokens via bare multisig"),
        ("Counterparty",   stats.counterparty_count,   stats.counterparty_vsize,  "asset protocol (XCP)"),
        ("Omni Layer",     stats.omni_count,           stats.omni_vsize,          "token layer (ex-Mastercoin)"),
        ("OP_RETURN other", stats.opreturn_other_count, stats.opreturn_other_vsize, "unclassified nulldata"),
        ("Other",          stats.other_data_count,     stats.other_data_vsize,    "data tx, unknown protocol"),
    ];

    let mut text = vec![
        Line::from(vec![
            Span::styled("Total output:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(format_btc(stats.total_out), Style::default().fg(Color::White).bold()),
        ]),
        Line::from(vec![
            Span::styled("Total fees:      ", Style::default().fg(Color::DarkGray)),
            Span::styled(format_btc_fees(stats.total_fee), Style::default().fg(Color::White).bold()),
        ]),
        Line::from(vec![
            Span::styled("Transactions:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{}", user_tx), Style::default().fg(Color::White).bold()),
            Span::styled("  (excl. coinbase)", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled("Total weight:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} WU ({:.1}%)", format_number(block.weight), block.weight as f64 / 4_000_000.0 * 100.0),
                Style::default().fg(Color::White).bold(),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled("Transaction Breakdown", Style::default().fg(Color::Cyan).bold())),
        Line::from(vec![
            Span::styled("  Financial:       ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} ({:.1}%)", stats.financial_count, fin_pct),
                Style::default().fg(Color::Green).bold(),
            ),
            Span::styled(
                format!("  {:>5}% wt", format!("{:.1}", if stats.total_vsize > 0 { stats.financial_vsize as f64 / stats.total_vsize as f64 * 100.0 } else { 0.0 })),
                Style::default().fg(Color::Green),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Data/spam:       ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} ({:.1}%)", data_count, data_pct),
                Style::default().fg(if data_pct > 10.0 { Color::Red } else { Color::Yellow }).bold(),
            ),
            Span::styled(
                format!("  {:>5}% wt", format!("{:.1}", if stats.total_vsize > 0 { data_vsize as f64 / stats.total_vsize as f64 * 100.0 } else { 0.0 })),
                Style::default().fg(if data_pct > 10.0 { Color::Red } else { Color::Yellow }),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled("Protocol Breakdown", Style::default().fg(Color::Cyan).bold())),
    ];
    let vsize_pct = |vs: u64| -> f64 {
        if stats.total_vsize > 0 { (vs as f64 / stats.total_vsize as f64) * 100.0 } else { 0.0 }
    };
    for (label, count, vsize, desc) in &protocols {
        text.push(Line::from(vec![
            Span::styled(format!("  {:17}", format!("{}:", label)), Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{:>4} ({:.1}%)", count, pct(*count)),
                Style::default().fg(proto_color(*count)),
            ),
            Span::styled(
                format!("  {:>5}% wt", format!("{:.1}", vsize_pct(*vsize))),
                Style::default().fg(proto_color(*count)),
            ),
            Span::styled(format!("  {}", desc), Style::default().fg(Color::DarkGray)),
        ]));
    }
    text.extend_from_slice(&[
        Line::from(""),
        Line::from(Span::styled("Taproot Usage", Style::default().fg(Color::Cyan).bold())),
        Line::from(vec![
            Span::styled("  Spending from:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} ({:.1}%)", stats.taproot_spend_count, taproot_spend_pct),
                Style::default().fg(proto_color(stats.taproot_spend_count)),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Creating to:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} ({:.1}%)", stats.taproot_output_count, taproot_output_pct),
                Style::default().fg(proto_color(stats.taproot_output_count)),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled("BIP-110 Compliance", Style::default().fg(Color::Cyan).bold())),
        Line::from(vec![
            Span::styled("  Compliant txs:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} ({:.1}%)", user_tx.saturating_sub(stats.bip110_violating_txs),
                    if user_tx > 0 { (user_tx.saturating_sub(stats.bip110_violating_txs)) as f64 / user_tx as f64 * 100.0 } else { 100.0 }),
                Style::default().fg(if stats.bip110_violating_txs == 0 { Color::Green } else { Color::Yellow }).bold(),
            ),
        ]),
        Line::from(Span::styled("  Violations:", Style::default().fg(Color::DarkGray))),
        Line::from(vec![
            Span::styled("    OP_RETURN >83B:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", stats.oversized_opreturn_count),
                Style::default().fg(viol_color(stats.oversized_opreturn_count)),
            ),
            Span::styled("  Largest: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if stats.max_opreturn_size > 0 { format!("{} bytes", stats.max_opreturn_size) } else { "n/a".to_string() },
                Style::default().fg(if stats.max_opreturn_size > 83 { Color::Red } else { Color::White }),
            ),
        ]),
        Line::from(vec![
            Span::styled("    scriptPubKey>34B:", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" {}", stats.bip110_oversized_spk),
                Style::default().fg(viol_color(stats.bip110_oversized_spk)),
            ),
        ]),
        Line::from(vec![
            Span::styled("    Witness >256B:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", stats.bip110_oversized_pushdata),
                Style::default().fg(viol_color(stats.bip110_oversized_pushdata)),
            ),
        ]),
        Line::from(vec![
            Span::styled("    OP_SUCCESS:      ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", stats.bip110_op_success),
                Style::default().fg(viol_color(stats.bip110_op_success)),
            ),
        ]),
        Line::from(vec![
            Span::styled("    OP_IF in tapscript:", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" {}", stats.bip110_op_if),
                Style::default().fg(viol_color(stats.bip110_op_if)),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "↑/↓: prev/next block | Esc: close",
            Style::default().fg(Color::DarkGray),
        )),
    ]);

    let title = format!(" Block {} ", format_number(block.height));
    let modal_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .title(title)
        .title_style(Style::default().fg(Color::Cyan).bold())
        .style(Style::default().bg(Color::Black));

    let paragraph = Paragraph::new(text)
        .block(modal_block)
        .wrap(Wrap { trim: false });

    f.render_widget(Clear, modal_area);
    f.render_widget(paragraph, modal_area);
}

fn draw_analytics(f: &mut Frame, area: Rect, analytics: &AnalyticsData) {
    match analytics.state {
        AnalyticsState::Idle => {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Analytics ")
                .style(Style::default().fg(Color::Cyan));
            let text = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "Waiting for dashboard data...",
                    Style::default().fg(Color::White),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Block analysis starts automatically after first dashboard fetch.",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(Span::styled(
                    "Results are saved to ~/.knots-tui/blockstats.jsonl",
                    Style::default().fg(Color::DarkGray),
                )),
            ])
            .block(block)
            .alignment(Alignment::Center);
            f.render_widget(text, area);
        }
        AnalyticsState::Running => {
            let pct = if analytics.progress_total > 0 {
                (analytics.progress_current as f64 / analytics.progress_total as f64 * 100.0) as u16
            } else {
                0
            };
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(4)])
                .split(area);

            let gauge_block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Analytics — Fetching ")
                .style(Style::default().fg(Color::Yellow));
            let gauge = Gauge::default()
                .block(gauge_block)
                .gauge_style(Style::default().fg(Color::Yellow).bg(Color::DarkGray))
                .percent(pct)
                .label(format!(
                    "{} / {} blocks ({}%)",
                    analytics.progress_current, analytics.progress_total, pct
                ));
            f.render_widget(gauge, rows[0]);

            if !analytics.stats.is_empty() {
                render_analytics_table(f, rows[1], &analytics.stats, analytics.missing_blocks, analytics.scroll);
            }
        }
        AnalyticsState::Done => {
            if analytics.stats.is_empty() {
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" Analytics ")
                    .style(Style::default().fg(Color::Green));
                let text = Paragraph::new("No data available.")
                    .block(block)
                    .alignment(Alignment::Center);
                f.render_widget(text, area);
            } else {
                render_analytics_table(f, area, &analytics.stats, analytics.missing_blocks, analytics.scroll);
            }
        }
    }
}

struct DayAgg {
    blocks: u64,
    txs: u64,
    financial: u64,
    runes: u64,
    brc20: u64,
    inscriptions: u64,
    opnet: u64,
    stamps: u64,
    counterparty: u64,
    omni: u64,
    opreturn_other: u64,
    oversized_opreturn: u64,
    bip110_violating_txs: u64,
    total_vsize: u64,
    financial_vsize: u64,
    rune_vsize: u64,
    brc20_vsize: u64,
    inscription_vsize: u64,
    opnet_vsize: u64,
    stamp_vsize: u64,
    counterparty_vsize: u64,
    omni_vsize: u64,
    opreturn_other_vsize: u64,
}

impl DayAgg {
    fn new() -> Self {
        DayAgg {
            blocks: 0, txs: 0, financial: 0, runes: 0, brc20: 0, inscriptions: 0,
            opnet: 0, stamps: 0, counterparty: 0, omni: 0, opreturn_other: 0,
            oversized_opreturn: 0, bip110_violating_txs: 0,
            total_vsize: 0, financial_vsize: 0, rune_vsize: 0, brc20_vsize: 0,
            inscription_vsize: 0, opnet_vsize: 0, stamp_vsize: 0, counterparty_vsize: 0,
            omni_vsize: 0, opreturn_other_vsize: 0,
        }
    }

    fn add(&mut self, s: &BlockStats) {
        self.blocks += 1;
        let user_tx = s.tx_count.saturating_sub(1) as u64;
        self.txs += user_tx;
        self.financial += s.financial_count as u64;
        self.runes += s.rune_count as u64;
        self.brc20 += s.brc20_count as u64;
        self.inscriptions += s.inscription_count as u64;
        self.opnet += s.opnet_count as u64;
        self.stamps += s.stamp_count as u64;
        self.counterparty += s.counterparty_count as u64;
        self.omni += s.omni_count as u64;
        self.opreturn_other += s.opreturn_other_count as u64;
        self.oversized_opreturn += s.oversized_opreturn_count as u64;
        self.bip110_violating_txs += s.bip110_violating_txs as u64;
        self.total_vsize += s.total_vsize;
        self.financial_vsize += s.financial_vsize;
        self.rune_vsize += s.rune_vsize;
        self.brc20_vsize += s.brc20_vsize;
        self.inscription_vsize += s.inscription_vsize;
        self.opnet_vsize += s.opnet_vsize;
        self.stamp_vsize += s.stamp_vsize;
        self.counterparty_vsize += s.counterparty_vsize;
        self.omni_vsize += s.omni_vsize;
        self.opreturn_other_vsize += s.opreturn_other_vsize;
    }
}

fn pct_str(n: u64, total: u64) -> String {
    if total > 0 { format!("{:.1}", n as f64 / total as f64 * 100.0) } else { "0.0".into() }
}

fn analytics_widths() -> [Constraint; 23] {
    [
        Constraint::Length(10), // Date/Label
        Constraint::Length(4),  // Blks
        Constraint::Length(7),  // TXs
        Constraint::Length(5),  // Fin%
        Constraint::Length(5),  // FinSz
        Constraint::Length(5),  // Dat%
        Constraint::Length(5),  // DatSz
        Constraint::Length(1),  // |
        Constraint::Length(6),  // Rune count
        Constraint::Length(5),  // Rune %
        Constraint::Length(6),  // Insc count
        Constraint::Length(5),  // Insc %
        Constraint::Length(6),  // BRC count
        Constraint::Length(5),  // BRC %
        Constraint::Length(6),  // OPN count
        Constraint::Length(5),  // OPN %
        Constraint::Length(6),  // Stp count
        Constraint::Length(5),  // Stp %
        Constraint::Length(6),  // OPR count
        Constraint::Length(5),  // OPR %
        Constraint::Length(1),  // |
        Constraint::Length(6),  // !110 count
        Constraint::Min(5),    // !110%
    ]
}

fn analytics_header_row() -> Row<'static> {
    let hdr = Style::default().fg(Color::Cyan).bold();
    let hdr_detail = Style::default().fg(Color::LightMagenta).bold();
    Row::new(vec![
        Cell::from("").style(hdr),
        Cell::from("Blks").style(hdr),
        Cell::from("TXs").style(hdr),
        Cell::from("Fin%").style(hdr),
        Cell::from("FinSz").style(hdr),
        Cell::from("Dat%").style(hdr),
        Cell::from("DatSz").style(hdr),
        Cell::from("|").style(Style::default().fg(Color::DarkGray)),
        Cell::from(format!("{:>6}", "Rune")).style(hdr_detail),
        Cell::from("%").style(hdr_detail),
        Cell::from(format!("{:>6}", "Insc")).style(hdr_detail),
        Cell::from("%").style(hdr_detail),
        Cell::from(format!("{:>6}", "BRC")).style(hdr_detail),
        Cell::from("%").style(hdr_detail),
        Cell::from(format!("{:>6}", "OPN")).style(hdr_detail),
        Cell::from("%").style(hdr_detail),
        Cell::from(format!("{:>6}", "Stp")).style(hdr_detail),
        Cell::from("%").style(hdr_detail),
        Cell::from(format!("{:>6}", "OPR")).style(hdr_detail),
        Cell::from("%").style(hdr_detail),
        Cell::from("|").style(Style::default().fg(Color::DarkGray)),
        Cell::from(Line::from("!110").alignment(Alignment::Right)).style(Style::default().fg(Color::Red).bold()),
        Cell::from("%").style(Style::default().fg(Color::Red).bold()),
    ])
}

fn analytics_data_row(label: &str, d: &DayAgg) -> Row<'static> {
    let sep = || Cell::from("|").style(Style::default().fg(Color::DarkGray));
    let detail_color = Color::LightMagenta;
    let data_tx = d.txs.saturating_sub(d.financial);
    let data_vsize = d.total_vsize.saturating_sub(d.financial_vsize);
    let mut cells = vec![
        Cell::from(label.to_string()).style(Style::default().fg(Color::White)),
        Cell::from(format!("{:>4}", format_compact(d.blocks))).style(Style::default().fg(Color::DarkGray)),
        Cell::from(format!("{:>7}", format_compact(d.txs))).style(Style::default().fg(Color::White)),
        Cell::from(format!("{}%", pct_str(d.financial, d.txs))).style(Style::default().fg(Color::Green)),
        Cell::from(format!("{}%", pct_str(d.financial_vsize, d.total_vsize))).style(Style::default().fg(Color::Green)),
        Cell::from(format!("{}%", pct_str(data_tx, d.txs))).style(Style::default().fg(if data_tx > 0 { Color::Yellow } else { Color::DarkGray })),
        Cell::from(format!("{}%", pct_str(data_vsize, d.total_vsize))).style(Style::default().fg(if data_vsize > 0 { Color::Yellow } else { Color::DarkGray })),
        sep(),
    ];
    let protos: Vec<u64> = vec![
        d.runes, d.inscriptions, d.brc20, d.opnet, d.stamps, d.opreturn_other,
    ];
    for count in protos {
        let c = if count > 0 { detail_color } else { Color::DarkGray };
        cells.push(Cell::from(format!("{:>6}", format_compact(count))).style(Style::default().fg(c)));
        cells.push(Cell::from(format!("{}%", pct_str(count, data_tx))).style(Style::default().fg(c)));
    }
    let viol_pct = if d.txs > 0 { d.bip110_violating_txs as f64 / d.txs as f64 * 100.0 } else { 0.0 };
    let bip110_color = if d.bip110_violating_txs == 0 { Color::Green } else if viol_pct <= 1.0 { Color::Yellow } else { Color::Red };
    cells.push(sep());
    cells.push(Cell::from(Line::from(format!("{}", format_compact(d.bip110_violating_txs))).alignment(Alignment::Right)).style(Style::default().fg(bip110_color)));
    let pct_cell = if d.bip110_violating_txs > 0 { format!("{:.1}%", viol_pct) } else { String::new() };
    cells.push(Cell::from(pct_cell).style(Style::default().fg(bip110_color)));
    Row::new(cells)
}

/// Aggregate stats within a time window into a single DayAgg
fn aggregate_period(stats: &[BlockStats], min_time: u64) -> DayAgg {
    let mut agg = DayAgg::new();
    for s in stats {
        if s.time >= min_time {
            agg.add(s);
        }
    }
    agg
}

fn render_analytics_table(f: &mut Frame, area: Rect, stats: &[BlockStats], missing: u64, scroll: u16) {
    // Aggregate by day
    let mut daily: BTreeMap<String, DayAgg> = BTreeMap::new();
    for s in stats {
        let date = chrono::DateTime::from_timestamp(s.time as i64, 0)
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        daily.entry(date).or_insert_with(DayAgg::new).add(s);
    }

    // Build rows newest first
    let rows: Vec<Row> = daily.iter().rev().map(|(date, d)| {
        analytics_data_row(date, d)
    }).collect();

    let block_count = stats.len();
    let table = Table::new(rows, analytics_widths())
        .header(analytics_header_row())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(if missing > 0 {
                    format!(" Daily Breakdown — {} blocks ({} missing) ", block_count, missing)
                } else {
                    format!(" Daily Breakdown — {} blocks ", block_count)
                })
                .style(Style::default().fg(Color::Cyan)),
        )
        .row_highlight_style(Style::default());

    let mut state = TableState::default().with_offset(scroll as usize);
    f.render_stateful_widget(table, area, &mut state);
}

fn format_compact(n: u64) -> String {
    if n < 1_000 {
        format!("{}", n)
    } else if n < 10_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else if n < 1_000_000 {
        format!("{}k", n / 1_000)
    } else if n < 10_000_000 {
        format!("{:.1}m", n as f64 / 1_000_000.0)
    } else {
        format!("{}m", n / 1_000_000)
    }
}

fn draw_footer(f: &mut Frame, area: Rect, screen: Screen, rpc_spinner: u8) {
    let hints = match screen {
        Screen::Dashboard => " q: quit | Tab: switch screen | j/k: switch table | ↑/↓: navigate | r: refresh ",
        Screen::KnownPeers => " q: quit | Tab: signaling | ↑/↓: scroll services | r: refresh ",
        Screen::Signaling => " q: quit | Tab: analytics | ↑/↓: select bit | Enter: details | r: refresh ",
        Screen::Analytics => " q: quit | Tab: dashboard | ↑/↓: scroll | +: extend 30 days | Esc: stop ",
    };

    const SPINNER: &[&str] = &[".  ", ".. ", "...", " ..", "  .", "   "];
    let spinner_str = SPINNER[rpc_spinner as usize % SPINNER.len()];

    let footer = Paragraph::new(Line::from(vec![
        Span::styled(hints, Style::default().fg(Color::DarkGray)),
        Span::styled(format!("[{}]", spinner_str), Style::default().fg(Color::DarkGray)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(footer, area);
}

// --- Formatting helpers ---

fn format_btc(satoshis: u64) -> String {
    let btc = satoshis as f64 / 100_000_000.0;
    if btc >= 1000.0 {
        format!("{:.0} BTC", btc)
    } else {
        format!("{:.2} BTC", btc)
    }
}

fn format_btc_fees(satoshis: u64) -> String {
    let btc = satoshis as f64 / 100_000_000.0;
    format!("{:.3} BTC", btc)
}

fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;

    let b = bytes as f64;
    if b >= TB {
        format!("{:.2} TB", b / TB)
    } else if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

fn format_bytes_short(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let b = bytes as f64;
    if b >= GB {
        format!("{:.1}G", b / GB)
    } else if b >= MB {
        format!("{:.1}M", b / MB)
    } else if b >= KB {
        format!("{:.0}K", b / KB)
    } else {
        format!("{}B", bytes)
    }
}

fn format_duration(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let mins = (seconds % 3600) / 60;

    if days > 0 {
        format!("{}d {}h {}m", days, hours, mins)
    } else if hours > 0 {
        format!("{}h {}m", hours, mins)
    } else {
        format!("{}m", mins)
    }
}

fn format_hashrate(hps: f64) -> String {
    if hps >= 1e18 {
        format!("~{:.2} EH/s", hps / 1e18)
    } else if hps >= 1e15 {
        format!("~{:.2} PH/s", hps / 1e15)
    } else if hps >= 1e12 {
        format!("~{:.2} TH/s", hps / 1e12)
    } else if hps >= 1e9 {
        format!("~{:.2} GH/s", hps / 1e9)
    } else {
        format!("~{:.2} H/s", hps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- format_compact ---

    #[test]
    fn compact_below_1000() {
        assert_eq!(format_compact(0), "0");
        assert_eq!(format_compact(999), "999");
    }

    #[test]
    fn compact_at_1000() {
        assert_eq!(format_compact(1000), "1.0k");
    }

    #[test]
    fn compact_below_10000() {
        assert_eq!(format_compact(1400), "1.4k");
        assert_eq!(format_compact(9999), "10.0k");
    }

    #[test]
    fn compact_at_10000() {
        assert_eq!(format_compact(10000), "10k");
        assert_eq!(format_compact(52000), "52k");
        assert_eq!(format_compact(999999), "999k");
    }

    #[test]
    fn compact_at_million() {
        assert_eq!(format_compact(1000000), "1.0m");
        assert_eq!(format_compact(1200000), "1.2m");
        assert_eq!(format_compact(9999999), "10.0m");
    }

    #[test]
    fn compact_above_10m() {
        assert_eq!(format_compact(10000000), "10m");
        assert_eq!(format_compact(50000000), "50m");
    }

    // --- format_btc ---

    #[test]
    fn btc_zero() {
        assert_eq!(format_btc(0), "0.00 BTC");
    }

    #[test]
    fn btc_one() {
        assert_eq!(format_btc(100_000_000), "1.00 BTC");
    }

    #[test]
    fn btc_above_1000() {
        assert_eq!(format_btc(100_000_000_000), "1000 BTC");
    }

    // --- format_btc_fees ---

    #[test]
    fn btc_fees_zero() {
        assert_eq!(format_btc_fees(0), "0.000 BTC");
    }

    #[test]
    fn btc_fees_typical() {
        assert_eq!(format_btc_fees(12345678), "0.123 BTC");
    }

    // --- format_number ---

    #[test]
    fn number_zero() {
        assert_eq!(format_number(0), "0");
    }

    #[test]
    fn number_below_1000() {
        assert_eq!(format_number(999), "999");
    }

    #[test]
    fn number_at_1000() {
        assert_eq!(format_number(1000), "1,000");
    }

    #[test]
    fn number_million() {
        assert_eq!(format_number(1000000), "1,000,000");
    }

    // --- format_bytes ---

    #[test]
    fn bytes_zero() {
        assert_eq!(format_bytes(0), "0 B");
    }

    #[test]
    fn bytes_kb() {
        assert_eq!(format_bytes(1024), "1.0 KB");
    }

    #[test]
    fn bytes_mb() {
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
    }

    #[test]
    fn bytes_gb() {
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn bytes_tb() {
        assert_eq!(format_bytes(1024u64 * 1024 * 1024 * 1024), "1.00 TB");
    }

    // --- format_bytes_short ---

    #[test]
    fn bytes_short_zero() {
        assert_eq!(format_bytes_short(0), "0B");
    }

    #[test]
    fn bytes_short_kb() {
        assert_eq!(format_bytes_short(1024), "1K");
    }

    #[test]
    fn bytes_short_mb() {
        assert_eq!(format_bytes_short(1024 * 1024), "1.0M");
    }

    #[test]
    fn bytes_short_gb() {
        assert_eq!(format_bytes_short(1024 * 1024 * 1024), "1.0G");
    }

    // --- format_duration ---

    #[test]
    fn duration_zero() {
        assert_eq!(format_duration(0), "0m");
    }

    #[test]
    fn duration_minutes() {
        assert_eq!(format_duration(300), "5m");
    }

    #[test]
    fn duration_hours() {
        assert_eq!(format_duration(3661), "1h 1m");
    }

    #[test]
    fn duration_days() {
        assert_eq!(format_duration(90061), "1d 1h 1m");
    }

    // --- format_hashrate ---

    #[test]
    fn hashrate_gh() {
        assert_eq!(format_hashrate(1e9), "~1.00 GH/s");
    }

    #[test]
    fn hashrate_th() {
        assert_eq!(format_hashrate(1e12), "~1.00 TH/s");
    }

    #[test]
    fn hashrate_ph() {
        assert_eq!(format_hashrate(1e15), "~1.00 PH/s");
    }

    #[test]
    fn hashrate_eh() {
        assert_eq!(format_hashrate(1e18), "~1.00 EH/s");
    }

    #[test]
    fn hashrate_low() {
        assert_eq!(format_hashrate(1000.0), "~1000.00 H/s");
    }

    // --- service_bit_name ---

    #[test]
    fn service_bit_known() {
        assert_eq!(service_bit_name(0), "NODE_NETWORK");
        assert_eq!(service_bit_name(3), "NODE_WITNESS");
        assert_eq!(service_bit_name(10), "NODE_NETWORK_LIMITED");
        assert_eq!(service_bit_name(24), "NODE_P2P_V2");
    }

    #[test]
    fn service_bit_unknown() {
        assert_eq!(service_bit_name(5), "");
        assert_eq!(service_bit_name(63), "");
    }

    // --- bit_detail ---

    #[test]
    fn bit_detail_csv() {
        let (title, _desc) = bit_detail(0);
        assert!(title.contains("csv"));
    }

    #[test]
    fn bit_detail_segwit() {
        let (title, _desc) = bit_detail(1);
        assert!(title.contains("segwit"));
    }

    #[test]
    fn bit_detail_taproot() {
        let (title, _desc) = bit_detail(2);
        assert!(title.contains("taproot"));
    }

    #[test]
    fn bit_detail_bip110() {
        let (title, _desc) = bit_detail(4);
        assert!(title.contains("BIP110"));
    }

    #[test]
    fn bit_detail_unassigned() {
        let (title, _desc) = bit_detail(3);
        assert!(title.contains("Unassigned"));
    }

    #[test]
    fn bit_detail_asicboost() {
        let (title, _desc) = bit_detail(13);
        assert!(title.contains("ASICBoost"));
        let (title2, _) = bit_detail(28);
        assert!(title2.contains("ASICBoost"));
    }
}
