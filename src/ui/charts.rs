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
    chart_mode: u8,
}

impl ChartsScreen {
    pub fn new(svc: Arc<AppService>, state: StateRef) -> Self {
        Self { svc, state, chart_mode: 0 }
    }
}

impl Screen for ChartsScreen {
    fn name(&self) -> &str {
        match self.chart_mode {
            0 => "Charts: OPNET",
            1 => "Charts: Data",
            _ => "Charts: BIP-110",
        }
    }

    fn footer_hint(&self) -> &str {
        " q: quit | Tab: dashboard | j/k: switch metric "
    }

    fn draw(&self, f: &mut Frame, area: Rect) {
        let state = self.state.borrow();
        draw_charts(f, area, &state.analytics, self.chart_mode);
    }

    fn handle_key(&mut self, key: KeyCode) -> KeyResult {
        match key {
            KeyCode::Char('j') => { self.chart_mode = (self.chart_mode + 1) % 3; KeyResult::None }
            KeyCode::Char('k') => { self.chart_mode = self.chart_mode.checked_sub(1).unwrap_or(2); KeyResult::None }
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

fn draw_charts(f: &mut Frame, area: Rect, analytics: &crate::AnalyticsData, chart_mode: u8) {
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

    let (daily_primary, hourly_primary) = match chart_mode {
        0 => (
            chart_aggregate_daily(stats, |s| (s.opnet_vsize, s.total_vsize)),
            chart_aggregate_hourly(stats, |s| (s.opnet_vsize, s.total_vsize)),
        ),
        1 => (
            chart_aggregate_daily(stats, |s| (s.total_vsize.saturating_sub(s.financial_vsize), s.total_vsize)),
            chart_aggregate_hourly(stats, |s| (s.total_vsize.saturating_sub(s.financial_vsize), s.total_vsize)),
        ),
        _ => (
            chart_aggregate_daily(stats, |s| (s.bip110_violating_vsize, s.total_vsize)),
            chart_aggregate_hourly(stats, |s| (s.bip110_violating_vsize, s.total_vsize)),
        ),
    };

    let daily_secondary = if chart_mode == 2 {
        chart_aggregate_daily(stats, |s| (s.bip110_violating_txs as u64, s.tx_count.saturating_sub(1) as u64))
    } else { Vec::new() };
    let hourly_secondary = if chart_mode == 2 {
        chart_aggregate_hourly(stats, |s| (s.bip110_violating_txs as u64, s.tx_count.saturating_sub(1) as u64))
    } else { Vec::new() };

    let num_days = daily_primary.len();
    let (primary_label, primary_color): (&str, Color) = match chart_mode {
        0 | 1 => ("weight%", Color::Yellow),
        _ => ("weight%", Color::Red),
    };

    let top_title = match chart_mode {
        0 => format!(" OPNET % of Block Weight — daily ({} days) ", num_days),
        1 => format!(" Data % of Block Weight — daily ({} days) ", num_days),
        _ => format!(" BIP-110 Non-Compliant — daily ({} days) ", num_days),
    };
    let bottom_title = match chart_mode {
        0 => " OPNET % of Block Weight — 24h Rolling Window (hourly) ",
        1 => " Data % of Block Weight — 24h Rolling Window (hourly) ",
        _ => " BIP-110 Non-Compliant — 24h Rolling Window (hourly) ",
    };

    let fmt_date = |ts: f64| -> String {
        chrono::DateTime::from_timestamp(ts as i64, 0)
            .map(|dt| dt.format("%m-%d").to_string())
            .unwrap_or_default()
    };
    let fmt_date_hour = |ts: f64| -> String {
        chrono::DateTime::from_timestamp(ts as i64, 0)
            .map(|dt| dt.format("%d %H:%M").to_string())
            .unwrap_or_default()
    };
    let y_labels = |top: f64, n: usize| -> Vec<Span<'static>> {
        (0..=n).map(|i| {
            let v = top * i as f64 / n as f64;
            if v == 0.0 { Span::raw("0") } else { Span::raw(format!("{:.1}", v)) }
        }).collect()
    };
    let x_labels_daily = |min: f64, max: f64, n: usize| -> Vec<Span<'static>> {
        (0..=n).map(|i| {
            let ts = min + (max - min) * i as f64 / n as f64;
            Span::raw(fmt_date(ts))
        }).collect()
    };
    let x_labels_hourly = |min: f64, max: f64, n: usize| -> Vec<Span<'static>> {
        (0..=n).map(|i| {
            let ts = min + (max - min) * i as f64 / n as f64;
            Span::raw(fmt_date_hour(ts))
        }).collect()
    };

    let d_min = daily_primary.first().map(|(x, _)| *x).unwrap_or(0.0);
    let d_max = daily_primary.last().map(|(x, _)| *x).unwrap_or(1.0);
    let d_range = if (d_max - d_min).abs() < 1.0 { [d_min - 86400.0, d_max + 86400.0] } else { [d_min, d_max] };
    let mut d_y_max = daily_primary.iter().map(|(_, y)| *y).fold(0.0f64, f64::max);
    if !daily_secondary.is_empty() { d_y_max = d_y_max.max(daily_secondary.iter().map(|(_, y)| *y).fold(0.0f64, f64::max)); }
    let d_y_top = (d_y_max * 1.1).max(0.5).min(100.0);

    let h_min = hourly_primary.first().map(|(x, _)| *x).unwrap_or(0.0);
    let h_max = hourly_primary.last().map(|(x, _)| *x).unwrap_or(1.0);
    let h_range = if (h_max - h_min).abs() < 1.0 { [h_min - 3600.0, h_max + 3600.0] } else { [h_min, h_max] };
    let mut h_y_max = hourly_primary.iter().map(|(_, y)| *y).fold(0.0f64, f64::max);
    if !hourly_secondary.is_empty() { h_y_max = h_y_max.max(hourly_secondary.iter().map(|(_, y)| *y).fold(0.0f64, f64::max)); }
    let h_y_top = (h_y_max * 1.1).max(0.5).min(100.0);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let mut top_datasets = vec![
        Dataset::default().name(primary_label).marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line).style(Style::default().fg(primary_color)).data(&daily_primary),
    ];
    if !daily_secondary.is_empty() {
        top_datasets.push(Dataset::default().name("tx count%").marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line).style(Style::default().fg(Color::Cyan)).data(&daily_secondary));
    }

    let chart_top = Chart::new(top_datasets)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(top_title).style(Style::default().fg(Color::Cyan)))
        .legend_position(Some(LegendPosition::TopLeft))
        .x_axis(Axis::default().style(Style::default().fg(Color::DarkGray)).bounds(d_range)
            .labels(x_labels_daily(d_min, d_max, 6)))
        .y_axis(Axis::default().title("%").style(Style::default().fg(Color::DarkGray)).bounds([0.0, d_y_top]).labels(y_labels(d_y_top, 5)));
    f.render_widget(chart_top, chunks[0]);

    let mut bottom_datasets = vec![
        Dataset::default().name(primary_label).marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line).style(Style::default().fg(primary_color)).data(&hourly_primary),
    ];
    if !hourly_secondary.is_empty() {
        bottom_datasets.push(Dataset::default().name("tx count%").marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line).style(Style::default().fg(Color::Cyan)).data(&hourly_secondary));
    }

    let chart_bottom = Chart::new(bottom_datasets)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(bottom_title).style(Style::default().fg(Color::Cyan)))
        .legend_position(Some(LegendPosition::TopLeft))
        .x_axis(Axis::default().style(Style::default().fg(Color::DarkGray)).bounds(h_range)
            .labels(x_labels_hourly(h_min, h_max, 6)))
        .y_axis(Axis::default().title("%").style(Style::default().fg(Color::DarkGray)).bounds([0.0, h_y_top]).labels(y_labels(h_y_top, 5)));
    f.render_widget(chart_bottom, chunks[1]);
}

fn chart_aggregate_daily(stats: &[BlockStats], extract: impl Fn(&BlockStats) -> (u64, u64)) -> Vec<(f64, f64)> {
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

fn chart_aggregate_hourly(stats: &[BlockStats], extract: impl Fn(&BlockStats) -> (u64, u64)) -> Vec<(f64, f64)> {
    let min_time = stats.iter().map(|s| s.time).min().unwrap_or(0);
    let max_time = stats.iter().map(|s| s.time).max().unwrap_or(0);
    let start_hour = (min_time / 3600) * 3600;
    let end_hour = ((max_time / 3600) + 1) * 3600;
    let mut points = Vec::new();
    let mut hour = start_hour;
    while hour <= end_hour {
        let window_start = hour.saturating_sub(86400);
        let mut total_num = 0u64;
        let mut total_den = 0u64;
        for s in stats {
            if s.time >= window_start && s.time <= hour {
                let (num, den) = extract(s);
                total_num += num; total_den += den;
            }
        }
        let pct = if total_den > 0 { total_num as f64 / total_den as f64 * 100.0 } else { 0.0 };
        points.push((hour as f64, pct));
        hour += 3600;
    }
    points
}
