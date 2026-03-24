pub mod analytics;
pub mod charts;
pub mod common;
pub mod dashboard;
pub mod ibd;
pub mod known_peers;
pub mod signaling;

use crate::rpc::{BlockStats, NodeData};
use crate::service::AppService;
use crate::sys::SystemStats;
use crate::{AnalyticsData, AnalyticsState};
use crossterm::event::KeyCode;
use ratatui::{prelude::*, widgets::*};
use std::collections::HashMap;

use common::format_duration;

pub struct SharedState {
    pub node_data: NodeData,
    pub signaling_data: NodeData,
    pub block_stats_cache: HashMap<u64, BlockStats>,
    pub analytics: AnalyticsData,
    pub system_stats: SystemStats,
    pub rpc_spinner: u8,
}

#[derive(PartialEq)]
pub enum KeyResult {
    None,
    Quit,
}

pub trait Screen {
    fn name(&self) -> &str;
    fn footer_hint(&self) -> &str;
    fn draw(&self, f: &mut Frame, area: Rect, state: &SharedState);
    fn handle_key(&mut self, key: KeyCode, state: &mut SharedState) -> KeyResult;

    fn has_modal(&self) -> bool { false }
    fn draw_modal(&self, _f: &mut Frame, _area: Rect, _state: &SharedState) {}
    fn handle_modal_key(&mut self, _key: KeyCode, _state: &mut SharedState) {}

    fn available(&self, _state: &SharedState) -> bool { true }

    /// Called when switching to this screen
    fn on_enter(&mut self) {}
}

pub fn draw(f: &mut Frame, screen: &dyn Screen, state: &SharedState, svc: &AppService) {
    let area = f.area();

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(1),
        ])
        .split(area);

    draw_header(f, outer[0], &state.node_data, screen.name(), svc.is_loading());
    screen.draw(f, outer[1], state);
    draw_footer(f, outer[2], screen.footer_hint(), state.rpc_spinner);

    if screen.has_modal() {
        screen.draw_modal(f, area, state);
    }
}

fn draw_header(f: &mut Frame, area: Rect, data: &NodeData, screen_name: &str, loading: bool) {
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
    let loading_str = if loading { " (loading...)" } else { "" };

    let title = format!(
        " Bitcoin Knots {}{} | {} | chain: {} | uptime: {} ",
        screen_name, loading_str, version, chain, uptime
    );

    let border_color = if loading { Color::Yellow } else { Color::Cyan };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().fg(border_color));

    let header = Paragraph::new(title)
        .block(block)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::White).bold());

    f.render_widget(header, area);
}

fn draw_footer(f: &mut Frame, area: Rect, hint: &str, rpc_spinner: u8) {
    const SPINNER: &[&str] = &[".  ", ".. ", "...", " ..", "  .", "   "];
    let spinner_str = SPINNER[rpc_spinner as usize % SPINNER.len()];

    let footer = Paragraph::new(Line::from(vec![
        Span::styled(hint, Style::default().fg(Color::DarkGray)),
        Span::styled(format!("[{}]", spinner_str), Style::default().fg(Color::DarkGray)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(footer, area);
}

pub fn next_screen(current: usize, screens: &[Box<dyn Screen>], state: &SharedState) -> usize {
    let n = screens.len();
    for i in 1..=n {
        let idx = (current + i) % n;
        if screens[idx].available(state) { return idx; }
    }
    current
}

pub fn prev_screen(current: usize, screens: &[Box<dyn Screen>], state: &SharedState) -> usize {
    let n = screens.len();
    for i in 1..=n {
        let idx = (current + n - i) % n;
        if screens[idx].available(state) { return idx; }
    }
    current
}
