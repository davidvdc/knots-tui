use crate::service::AppService;
use crate::sys::SystemStats;
use crossterm::event::KeyCode;
use ratatui::{prelude::*, widgets::*};
use std::sync::Arc;

use super::common::{format_bytes, format_bytes_short, format_duration, format_number};
use super::dashboard::draw_peers_table;
use super::{KeyResult, Screen, StateRef};

pub struct IbdScreen {
    svc: Arc<AppService>,
    state: StateRef,
    peer_scroll: u16,
}

impl IbdScreen {
    pub fn new(svc: Arc<AppService>, state: StateRef) -> Self {
        Self { svc, state, peer_scroll: 0 }
    }
}

impl Screen for IbdScreen {
    fn name(&self) -> &str { "Initial Block Download" }

    fn footer_hint(&self) -> &str {
        " q: quit | Tab: switch screen | ↑/↓: scroll peers | r: refresh "
    }

    fn draw(&self, f: &mut Frame, area: Rect) {
        let state = self.state.borrow();
        draw_ibd(f, area, &state.node_data, self.peer_scroll, &state.system_stats);
    }

    fn handle_key(&mut self, key: KeyCode) -> KeyResult {
        match key {
            KeyCode::Down => { self.peer_scroll = self.peer_scroll.saturating_add(1); KeyResult::None }
            KeyCode::Up => { self.peer_scroll = self.peer_scroll.saturating_sub(1); KeyResult::None }
            KeyCode::Char('r') => { self.svc.force_refresh(); KeyResult::None }
            KeyCode::Esc => KeyResult::Quit,
            _ => KeyResult::None,
        }
    }

    fn available(&self) -> bool {
        self.state.borrow().node_data.blockchain.initialblockdownload
    }

    fn on_enter(&mut self) {
        self.svc.set_loading(true);
        self.svc.start_polling();
    }
}

fn draw_ibd(f: &mut Frame, area: Rect, data: &crate::rpc::NodeData, peer_scroll: u16, sys: &SystemStats) {
    let bc = &data.blockchain;
    let net = &data.network;
    let progress = (bc.verificationprogress * 100.0).min(100.0);

    let bar_width = 40usize;
    let filled = ((bc.verificationprogress * bar_width as f64) as usize).min(bar_width);
    let bar = format!("[{}{}]", "#".repeat(filled), " ".repeat(bar_width - filled));

    let remaining_blocks = bc.headers.saturating_sub(bc.blocks);
    let eta = if data.ibd_blocks_per_sec > 0.1 {
        let secs = (remaining_blocks as f64 / data.ibd_blocks_per_sec) as u64;
        format!("~{}", format_duration(secs))
    } else { "-".to_string() };
    let speed = if data.ibd_blocks_per_sec > 0.1 {
        format!("{:.1} blk/s", data.ibd_blocks_per_sec)
    } else { "-".to_string() };
    let dl_rate = if data.ibd_recv_per_sec > 0 {
        format!("{}/s", format_bytes(data.ibd_recv_per_sec))
    } else { "-".to_string() };

    let gray = Style::default().fg(Color::DarkGray);
    let white = Style::default().fg(Color::White);
    let yellow = Style::default().fg(Color::Yellow).bold();

    let lines = vec![
        Line::from(vec![Span::styled("Progress: ", gray), Span::styled(bar, yellow), Span::styled(format!("  {:.2}%", progress), yellow)]),
        Line::from(vec![
            Span::styled("Synced:   ", gray),
            Span::styled(format!("{} / {} blocks", format_number(bc.blocks), format_number(bc.headers)), white),
            Span::styled("   tip: ", gray),
            Span::styled(if bc.time > 0 { chrono::DateTime::from_timestamp(bc.time as i64, 0).map(|dt| dt.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_else(|| "-".to_string()) } else { "-".to_string() }, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![Span::styled("Speed:    ", gray), Span::styled(&speed, Style::default().fg(Color::Cyan)), Span::styled("   ETA: ", gray), Span::styled(&eta, white)]),
        Line::from(vec![Span::styled("Peers:    ", gray), Span::styled(format!("{} (in: {} / out: {})", net.connections, net.connections_in, net.connections_out), white)]),
        Line::from(vec![Span::styled("Download: ", gray), Span::styled(&dl_rate, Style::default().fg(Color::Green)), Span::styled("   total recv: ", gray), Span::styled(format_bytes(data.net_totals.totalbytesrecv), white)]),
        Line::from(vec![Span::styled("Disk:     ", gray), Span::styled(format_bytes(bc.size_on_disk), white), Span::styled("   Difficulty: ", gray), Span::styled(format!("{:.2e}", bc.difficulty), white)]),
    ];

    let has_sys = !sys.cpus.is_empty() || sys.mem.total > 0;
    let has_swap = sys.mem.swap_total > 0;
    let disk_lines = sys.disks.len().min(8);
    let mem_lines = if sys.mem.total > 0 { 1 } else { 0 };
    let swap_lines = if has_swap { 1 } else { 0 };
    let sys_height = if has_sys { (sys.cpus.len() + mem_lines + swap_lines + disk_lines + 2) as u16 } else { 0 };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Length(sys_height), Constraint::Min(5)])
        .split(area);

    let block = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)
        .title(" Initial Block Download ").title_style(Style::default().fg(Color::Yellow).bold());
    f.render_widget(Paragraph::new(lines).block(block), rows[0]);

    if has_sys { draw_system_box(f, rows[1], sys); }

    draw_peers_table(f, rows[2], data, peer_scroll, false);
}

fn draw_system_box(f: &mut Frame, area: Rect, sys: &SystemStats) {
    let block = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)
        .title(" System ").title_style(Style::default().fg(Color::Cyan).bold());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let available_w = inner.width as usize;
    let label_w = if sys.cpus.is_empty() { 3 } else { format!("{}", sys.cpus.len() - 1).len().max(3) };

    let cpu_infos: Vec<String> = sys.cpus.iter().map(|cpu| {
        let total = (cpu.nice_pct + cpu.user_pct + cpu.system_pct + cpu.iowait_pct).min(100.0);
        format!("{:.0}%", total)
    }).collect();
    let mem_info = if sys.mem.total > 0 { format!("{}/{}", format_bytes_short(sys.mem.used + sys.mem.buffers + sys.mem.cached), format_bytes_short(sys.mem.total)) } else { String::new() };
    let swp_info = if sys.mem.swap_total > 0 { format!("{}/{}", format_bytes_short(sys.mem.swap_used), format_bytes_short(sys.mem.swap_total)) } else { String::new() };
    let max_info = cpu_infos.iter().map(|s| s.len()).chain(std::iter::once(mem_info.len())).chain(std::iter::once(swp_info.len())).max().unwrap_or(4);
    let bar_w = available_w.saturating_sub(4 + label_w + max_info).max(10);

    let mut lines: Vec<Line> = Vec::new();

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
        let mut spans = vec![Span::styled(format!(" {:>w$}", i, w = label_w), Style::default().fg(Color::Cyan)), Span::styled("[", Style::default().fg(Color::DarkGray))];
        if nice_c > 0 { spans.push(Span::styled("|".repeat(nice_c), Style::default().fg(Color::Blue))); }
        if user_c > 0 { spans.push(Span::styled("|".repeat(user_c), Style::default().fg(Color::Green))); }
        if sys_c > 0 { spans.push(Span::styled("|".repeat(sys_c), Style::default().fg(Color::Red))); }
        if iow_c > 0 { spans.push(Span::styled("|".repeat(iow_c), Style::default().fg(Color::DarkGray))); }
        spans.push(Span::raw(" ".repeat(empty_c)));
        spans.push(Span::styled("]", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(format!(" {:>w$}", &cpu_infos[i], w = max_info), Style::default().fg(Color::White)));
        lines.push(Line::from(spans));
    }

    if sys.mem.total > 0 {
        let used_c = (sys.mem.used as f64 / sys.mem.total as f64 * bar_w as f64).round() as usize;
        let buf_c = (sys.mem.buffers as f64 / sys.mem.total as f64 * bar_w as f64).round() as usize;
        let cache_c = (sys.mem.cached as f64 / sys.mem.total as f64 * bar_w as f64).round() as usize;
        let filled = (used_c + buf_c + cache_c).min(bar_w);
        let empty = bar_w.saturating_sub(filled);
        let mut spans = vec![Span::styled(format!(" {:>w$}", "Mem", w = label_w), Style::default().fg(Color::Cyan)), Span::styled("[", Style::default().fg(Color::DarkGray))];
        if used_c > 0 { spans.push(Span::styled("|".repeat(used_c), Style::default().fg(Color::Green))); }
        if buf_c > 0 { spans.push(Span::styled("|".repeat(buf_c), Style::default().fg(Color::Blue))); }
        if cache_c > 0 { spans.push(Span::styled("|".repeat(cache_c), Style::default().fg(Color::Yellow))); }
        spans.push(Span::raw(" ".repeat(empty)));
        spans.push(Span::styled("]", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(format!(" {:>w$}", &mem_info, w = max_info), Style::default().fg(Color::White)));
        lines.push(Line::from(spans));
    }

    if sys.mem.swap_total > 0 {
        let used_c = (sys.mem.swap_used as f64 / sys.mem.swap_total as f64 * bar_w as f64).round() as usize;
        let empty = bar_w.saturating_sub(used_c);
        let mut spans = vec![Span::styled(format!(" {:>w$}", "Swp", w = label_w), Style::default().fg(Color::Cyan)), Span::styled("[", Style::default().fg(Color::DarkGray))];
        if used_c > 0 { spans.push(Span::styled("|".repeat(used_c), Style::default().fg(Color::Red))); }
        spans.push(Span::raw(" ".repeat(empty)));
        spans.push(Span::styled("]", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(format!(" {:>w$}", &swp_info, w = max_info), Style::default().fg(Color::White)));
        lines.push(Line::from(spans));
    }

    let disk_name_w = sys.disks.iter().take(8).map(|d| d.name.len()).max().unwrap_or(3).max(label_w);
    for disk in sys.disks.iter().take(8) {
        lines.push(Line::from(vec![
            Span::styled(format!(" {:>w$}", disk.name, w = disk_name_w), Style::default().fg(Color::Cyan)),
            Span::styled("  R ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:>9}/s", format_bytes_short(disk.read_per_sec)), Style::default().fg(Color::Green)),
            Span::styled("  W ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:>9}/s", format_bytes_short(disk.write_per_sec)), Style::default().fg(Color::Yellow)),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}
