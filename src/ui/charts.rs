use crate::rpc::BlockStats;
use crate::service::AppService;
use crossterm::event::KeyCode;
use ratatui::{prelude::*, symbols, widgets::*};
use std::collections::BTreeMap;
use std::sync::Arc;

use super::{KeyResult, Screen, StateRef};

pub struct ChartsScreen {
    svc: Arc<AppService>,
    state: StateRef,
    time_mode: u8, // 0 = daily, 1 = rolling 24h
}

impl ChartsScreen {
    pub fn new(svc: Arc<AppService>, state: StateRef) -> Self {
        Self { svc, state, time_mode: 0 }
    }
}

impl Screen for ChartsScreen {
    fn name(&self) -> &str { "Charts" }

    fn footer_hint(&self) -> &str {
        " q: quit | Tab: dashboard | h/l: daily/rolling "
    }

    fn draw(&self, f: &mut Frame, area: Rect) {
        let state = self.state.borrow();
        draw_charts(f, area, &state.analytics, self.time_mode);
    }

    fn handle_key(&mut self, key: KeyCode) -> KeyResult {
        match key {
            KeyCode::Char('h') | KeyCode::Char('l') => { self.time_mode = 1 - self.time_mode; KeyResult::None }
            KeyCode::Esc => KeyResult::Quit,
            _ => KeyResult::None,
        }
    }

    fn available(&self) -> bool {
        !self.state.borrow().node_data.blockchain.initialblockdownload
    }

    fn on_enter(&mut self) {
        self.svc.stop_polling();
    }
}

// Protocol definitions in stacking order (bottom to top)
const PROTOS: &[(&str, Color)] = &[
    ("OP_RET",   Color::DarkGray),
    ("Omni",     Color::White),
    ("CNTRPRTY", Color::Cyan),
    ("Stamps",   Color::Green),
    ("OPNET",    Color::Red),
    ("BRC-20",   Color::Blue),
    ("Inscr",    Color::Magenta),
    ("Runes",    Color::Yellow),
];

fn extract_proto_vsize(s: &BlockStats, idx: usize) -> u64 {
    match idx {
        0 => s.opreturn_other_vsize,
        1 => s.omni_vsize,
        2 => s.counterparty_vsize,
        3 => s.stamp_vsize,
        4 => s.opnet_vsize,
        5 => s.brc20_vsize,
        6 => s.inscription_vsize,
        7 => s.rune_vsize,
        _ => 0,
    }
}

fn draw_charts(f: &mut Frame, area: Rect, analytics: &crate::AnalyticsData, time_mode: u8) {
    if analytics.stats.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Charts ")
            .style(Style::default().fg(Color::Cyan));
        let text = Paragraph::new("No data available.")
            .block(block)
            .alignment(Alignment::Center);
        f.render_widget(text, area);
        return;
    }

    let stats = &analytics.stats;
    let is_daily = time_mode == 0;
    let time_label = if is_daily { "daily" } else { "24h rolling" };

    // BIP-110 data
    let bip110_weight = agg(stats, |s| (s.bip110_violating_vsize, s.total_vsize), is_daily);
    let bip110_count = agg(stats, |s| (s.bip110_violating_txs as u64, s.tx_count.saturating_sub(1) as u64), is_daily);

    // Cumulative protocol data
    let proto_cum = build_cumulative_protos(stats, is_daily);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // --- Top: BIP-110 ---
    {
        let title = format!(" BIP-110 Non-Compliant -- {} ", time_label);
        let (x_min, x_max) = x_bounds(&bip110_weight);
        let pad = if is_daily { 86400.0 } else { 3600.0 };
        let x_range = if (x_max - x_min).abs() < 1.0 { [x_min - pad, x_max + pad] } else { [x_min, x_max] };
        let y_max = max_y(&bip110_weight).max(max_y(&bip110_count));
        let y_top = round_y_top(y_max);
        let datasets = vec![
            Dataset::default().name("weight%").marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line).style(Style::default().fg(Color::Red)).data(&bip110_weight),
            Dataset::default().name("tx count%").marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line).style(Style::default().fg(Color::Cyan)).data(&bip110_count),
        ];
        let chart = Chart::new(datasets)
            .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(title).style(Style::default().fg(Color::Cyan)))
            .legend_position(Some(LegendPosition::TopLeft))
            .x_axis(Axis::default().style(Style::default().fg(Color::DarkGray)).bounds(x_range).labels(make_x_labels(x_min, x_max, 6, is_daily)))
            .y_axis(Axis::default().style(Style::default().fg(Color::DarkGray)).bounds([0.0, y_top]).labels(make_y_labels(y_top)));
        f.render_widget(chart, chunks[0]);
    }

    // --- Bottom: cumulative protocol weight% ---
    {
        let title = format!(" Data Protocols -- cumulative weight% -- {} ", time_label);
        let top_idx = PROTOS.len() - 1;
        let (x_min, x_max) = if proto_cum[top_idx].is_empty() { (0.0, 1.0) } else { x_bounds(&proto_cum[top_idx]) };
        let pad = if is_daily { 86400.0 } else { 3600.0 };
        let x_range = if (x_max - x_min).abs() < 1.0 { [x_min - pad, x_max + pad] } else { [x_min, x_max] };
        let y_max = max_y(&proto_cum[top_idx]);
        let y_top = round_y_top(y_max);

        // Draw highest cumulative first so lower lines paint on top
        let mut datasets = Vec::new();
        for i in (0..PROTOS.len()).rev() {
            datasets.push(
                Dataset::default().name(PROTOS[i].0).marker(symbols::Marker::Braille)
                    .graph_type(GraphType::Line).style(Style::default().fg(PROTOS[i].1)).data(&proto_cum[i])
            );
        }
        let chart = Chart::new(datasets)
            .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(title).style(Style::default().fg(Color::Cyan)))
            .legend_position(Some(LegendPosition::TopLeft))
            .x_axis(Axis::default().style(Style::default().fg(Color::DarkGray)).bounds(x_range).labels(make_x_labels(x_min, x_max, 6, is_daily)))
            .y_axis(Axis::default().style(Style::default().fg(Color::DarkGray)).bounds([0.0, y_top]).labels(make_y_labels(y_top)));
        f.render_widget(chart, chunks[1]);
    }
}

fn round_y_top(y_max: f64) -> f64 {
    ((y_max * 1.1 / 5.0).ceil() * 5.0).max(5.0).min(100.0)
}

fn x_bounds(data: &[(f64, f64)]) -> (f64, f64) {
    (data.first().map(|(x, _)| *x).unwrap_or(0.0), data.last().map(|(x, _)| *x).unwrap_or(1.0))
}

fn max_y(data: &[(f64, f64)]) -> f64 {
    data.iter().map(|(_, y)| *y).fold(0.0f64, f64::max)
}

fn make_y_labels(top: f64) -> Vec<Span<'static>> {
    let n = (top / 5.0) as usize;
    (0..=n).map(|i| Span::raw(format!("{}%", i * 5))).collect()
}

fn make_x_labels(min: f64, max: f64, n: usize, daily: bool) -> Vec<Span<'static>> {
    (0..=n).map(|i| {
        let ts = min + (max - min) * i as f64 / n as f64;
        let s = chrono::DateTime::from_timestamp(ts as i64, 0)
            .map(|dt| if daily { dt.format("%m-%d").to_string() } else { dt.format("%d %H:%M").to_string() })
            .unwrap_or_default();
        Span::raw(s)
    }).collect()
}

fn agg(stats: &[BlockStats], extract: impl Fn(&BlockStats) -> (u64, u64), daily: bool) -> Vec<(f64, f64)> {
    if daily { agg_daily(stats, extract) } else { agg_hourly(stats, extract) }
}

fn build_cumulative_protos(stats: &[BlockStats], daily: bool) -> Vec<Vec<(f64, f64)>> {
    let buckets: Vec<(f64, Vec<u64>, u64)> = if daily { agg_daily_multi(stats) } else { agg_hourly_multi(stats) };
    let mut result: Vec<Vec<(f64, f64)>> = vec![Vec::with_capacity(buckets.len()); PROTOS.len()];
    for (ts, protos, total) in &buckets {
        let mut cum = 0.0;
        for i in 0..PROTOS.len() {
            cum += if *total > 0 { protos[i] as f64 / *total as f64 * 100.0 } else { 0.0 };
            result[i].push((*ts, cum));
        }
    }
    result
}

fn agg_daily_multi(stats: &[BlockStats]) -> Vec<(f64, Vec<u64>, u64)> {
    let mut daily: BTreeMap<u64, (Vec<u64>, u64)> = BTreeMap::new();
    for s in stats {
        let day = (s.time / 86400) * 86400;
        let entry = daily.entry(day).or_insert_with(|| (vec![0u64; PROTOS.len()], 0));
        for i in 0..PROTOS.len() { entry.0[i] += extract_proto_vsize(s, i); }
        entry.1 += s.total_vsize;
    }
    daily.into_iter().map(|(ts, (p, t))| (ts as f64, p, t)).collect()
}

fn agg_hourly_multi(stats: &[BlockStats]) -> Vec<(f64, Vec<u64>, u64)> {
    let min_time = stats.iter().map(|s| s.time).min().unwrap_or(0);
    let max_time = stats.iter().map(|s| s.time).max().unwrap_or(0);
    let start = (min_time / 3600) * 3600;
    let end = ((max_time / 3600) + 1) * 3600;
    let mut pts = Vec::new();
    let mut h = start;
    while h <= end {
        let ws = h.saturating_sub(86400);
        let mut protos = vec![0u64; PROTOS.len()];
        let mut total = 0u64;
        for s in stats {
            if s.time >= ws && s.time <= h {
                for i in 0..PROTOS.len() { protos[i] += extract_proto_vsize(s, i); }
                total += s.total_vsize;
            }
        }
        pts.push((h as f64, protos, total));
        h += 3600;
    }
    pts
}

fn agg_daily(stats: &[BlockStats], extract: impl Fn(&BlockStats) -> (u64, u64)) -> Vec<(f64, f64)> {
    let mut daily: BTreeMap<u64, (u64, u64)> = BTreeMap::new();
    for s in stats {
        let day = (s.time / 86400) * 86400;
        let (num, den) = extract(s);
        let entry = daily.entry(day).or_insert((0, 0));
        entry.0 += num; entry.1 += den;
    }
    daily.iter().map(|(ts, (num, den))| {
        let pct = if *den > 0 { *num as f64 / *den as f64 * 100.0 } else { 0.0 };
        (*ts as f64, pct)
    }).collect()
}

fn agg_hourly(stats: &[BlockStats], extract: impl Fn(&BlockStats) -> (u64, u64)) -> Vec<(f64, f64)> {
    let min_time = stats.iter().map(|s| s.time).min().unwrap_or(0);
    let max_time = stats.iter().map(|s| s.time).max().unwrap_or(0);
    let start = (min_time / 3600) * 3600;
    let end = ((max_time / 3600) + 1) * 3600;
    let mut pts = Vec::new();
    let mut h = start;
    while h <= end {
        let ws = h.saturating_sub(86400);
        let mut tn = 0u64; let mut td = 0u64;
        for s in stats {
            if s.time >= ws && s.time <= h {
                let (n, d) = extract(s);
                tn += n; td += d;
            }
        }
        pts.push((h as f64, if td > 0 { tn as f64 / td as f64 * 100.0 } else { 0.0 }));
        h += 3600;
    }
    pts
}
