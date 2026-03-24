use crate::rpc::BlockStats;
use crate::service::AppService;
use crate::{AnalyticsData, AnalyticsState};
use crossterm::event::KeyCode;
use ratatui::{prelude::*, widgets::*};
use std::collections::{BTreeMap, HashSet};

use super::common::{format_compact, format_number, pct_str};
use super::{KeyResult, Screen, SharedState};

pub struct AnalyticsScreen {
    scroll: u16,
}

impl AnalyticsScreen {
    pub fn new() -> Self {
        Self { scroll: 0 }
    }
}

impl Screen for AnalyticsScreen {
    fn name(&self) -> &str { "Analytics" }

    fn footer_hint(&self) -> &str {
        " q: quit | Tab: charts | ↑/↓: scroll | +: extend 30 days | Esc: stop "
    }

    fn draw(&self, f: &mut Frame, area: Rect, state: &SharedState) {
        let analytics = &state.analytics;
        match analytics.state {
            AnalyticsState::Idle => {
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" Analytics ")
                    .style(Style::default().fg(Color::Cyan));
                let text = Paragraph::new(vec![
                    Line::from(""),
                    Line::from(Span::styled("Waiting for dashboard data...", Style::default().fg(Color::White))),
                    Line::from(""),
                    Line::from(Span::styled("Block analysis starts automatically after first dashboard fetch.", Style::default().fg(Color::DarkGray))),
                    Line::from(Span::styled("Results are saved to ~/.knots-tui/blockstats.jsonl", Style::default().fg(Color::DarkGray))),
                ])
                .block(block)
                .alignment(Alignment::Center);
                f.render_widget(text, area);
            }
            AnalyticsState::Running => {
                let pct = if analytics.progress_total > 0 {
                    (analytics.progress_current as f64 / analytics.progress_total as f64 * 100.0) as u16
                } else { 0 };
                let rows = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(3), Constraint::Min(4)])
                    .split(area);

                let gauge_block = Block::default()
                    .borders(Borders::ALL).border_type(BorderType::Rounded)
                    .title(" Analytics — Fetching ")
                    .style(Style::default().fg(Color::Yellow));
                let gauge = Gauge::default()
                    .block(gauge_block)
                    .gauge_style(Style::default().fg(Color::Yellow).bg(Color::DarkGray))
                    .percent(pct)
                    .label(format!("{} / {} blocks ({}%)", analytics.progress_current, analytics.progress_total, pct));
                f.render_widget(gauge, rows[0]);

                if !analytics.stats.is_empty() {
                    render_analytics_table(f, rows[1], &analytics.stats, analytics.missing_blocks, self.scroll);
                }
            }
            AnalyticsState::Done => {
                if analytics.stats.is_empty() {
                    let block = Block::default()
                        .borders(Borders::ALL).border_type(BorderType::Rounded)
                        .title(" Analytics ")
                        .style(Style::default().fg(Color::Green));
                    let text = Paragraph::new("No data available.").block(block).alignment(Alignment::Center);
                    f.render_widget(text, area);
                } else {
                    render_analytics_table(f, area, &analytics.stats, analytics.missing_blocks, self.scroll);
                }
            }
        }
    }

    fn handle_key(&mut self, key: KeyCode, state: &mut SharedState, svc: &AppService) -> KeyResult {
        match key {
            KeyCode::Down => { self.scroll = self.scroll.saturating_add(1); KeyResult::None }
            KeyCode::Up => { self.scroll = self.scroll.saturating_sub(1); KeyResult::None }
            KeyCode::Esc => {
                if state.analytics.state == AnalyticsState::Running {
                    state.analytics.state = AnalyticsState::Done;
                    state.analytics.stats.sort_by_key(|s| s.height);
                    svc.stop_backfill();
                    KeyResult::None
                } else {
                    KeyResult::Quit
                }
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                if state.analytics.state != AnalyticsState::Running && !state.node_data.blockchain.initialblockdownload {
                    state.analytics.depth += 4320;
                    let tip = state.node_data.blockchain.blocks;
                    let start = tip.saturating_sub(state.analytics.depth);
                    let all_heights: Vec<u64> = (start..=tip).rev().collect();
                    let cached: HashSet<u64> = state.block_stats_cache.keys().copied().collect();
                    let total = svc.spawn_backfill(&[], all_heights, &cached);
                    if total > 0 {
                        state.analytics.state = AnalyticsState::Running;
                        state.analytics.progress_current = 0;
                        state.analytics.progress_total = total;
                        state.analytics.missing_blocks = total;
                    }
                }
                KeyResult::None
            }
            _ => KeyResult::None,
        }
    }

    fn available(&self, state: &SharedState) -> bool {
        !state.node_data.blockchain.initialblockdownload
    }
}

// --- Public types/functions used by dashboard's analytics summary ---

pub struct DayAgg {
    pub blocks: u64,
    pub txs: u64,
    pub financial: u64,
    pub runes: u64,
    pub brc20: u64,
    pub inscriptions: u64,
    pub opnet: u64,
    pub stamps: u64,
    pub counterparty: u64,
    pub omni: u64,
    pub opreturn_other: u64,
    pub oversized_opreturn: u64,
    pub bip110_violating_txs: u64,
    pub total_vsize: u64,
    pub financial_vsize: u64,
    pub rune_vsize: u64,
    pub brc20_vsize: u64,
    pub inscription_vsize: u64,
    pub opnet_vsize: u64,
    pub stamp_vsize: u64,
    pub counterparty_vsize: u64,
    pub omni_vsize: u64,
    pub opreturn_other_vsize: u64,
}

impl DayAgg {
    pub fn new() -> Self {
        DayAgg {
            blocks: 0, txs: 0, financial: 0, runes: 0, brc20: 0, inscriptions: 0,
            opnet: 0, stamps: 0, counterparty: 0, omni: 0, opreturn_other: 0,
            oversized_opreturn: 0, bip110_violating_txs: 0,
            total_vsize: 0, financial_vsize: 0, rune_vsize: 0, brc20_vsize: 0,
            inscription_vsize: 0, opnet_vsize: 0, stamp_vsize: 0, counterparty_vsize: 0,
            omni_vsize: 0, opreturn_other_vsize: 0,
        }
    }

    pub fn add(&mut self, s: &BlockStats) {
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

pub fn analytics_widths() -> [Constraint; 23] {
    [
        Constraint::Length(10), Constraint::Length(4), Constraint::Length(7),
        Constraint::Length(5), Constraint::Length(5), Constraint::Length(5), Constraint::Length(5),
        Constraint::Length(1),
        Constraint::Length(6), Constraint::Length(5), Constraint::Length(6), Constraint::Length(5),
        Constraint::Length(6), Constraint::Length(5), Constraint::Length(6), Constraint::Length(5),
        Constraint::Length(6), Constraint::Length(5), Constraint::Length(6), Constraint::Length(5),
        Constraint::Length(1), Constraint::Length(6), Constraint::Min(5),
    ]
}

pub fn analytics_header_row() -> Row<'static> {
    let hdr = Style::default().fg(Color::Cyan).bold();
    let hdr_detail = Style::default().fg(Color::LightMagenta).bold();
    Row::new(vec![
        Cell::from("").style(hdr),
        Cell::from("Blks").style(hdr), Cell::from("TXs").style(hdr),
        Cell::from("Fin%").style(hdr), Cell::from("FinSz").style(hdr),
        Cell::from("Dat%").style(hdr), Cell::from("DatSz").style(hdr),
        Cell::from("|").style(Style::default().fg(Color::DarkGray)),
        Cell::from(format!("{:>6}", "Rune")).style(hdr_detail), Cell::from("%").style(hdr_detail),
        Cell::from(format!("{:>6}", "Insc")).style(hdr_detail), Cell::from("%").style(hdr_detail),
        Cell::from(format!("{:>6}", "BRC")).style(hdr_detail), Cell::from("%").style(hdr_detail),
        Cell::from(format!("{:>6}", "OPN")).style(hdr_detail), Cell::from("%").style(hdr_detail),
        Cell::from(format!("{:>6}", "Stp")).style(hdr_detail), Cell::from("%").style(hdr_detail),
        Cell::from(format!("{:>6}", "OPR")).style(hdr_detail), Cell::from("%").style(hdr_detail),
        Cell::from("|").style(Style::default().fg(Color::DarkGray)),
        Cell::from(Line::from("!110").alignment(Alignment::Right)).style(Style::default().fg(Color::Red).bold()),
        Cell::from("%").style(Style::default().fg(Color::Red).bold()),
    ])
}

pub fn analytics_data_row(label: &str, d: &DayAgg) -> Row<'static> {
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
    let protos: Vec<u64> = vec![d.runes, d.inscriptions, d.brc20, d.opnet, d.stamps, d.opreturn_other];
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

pub fn aggregate_period(stats: &[BlockStats], min_time: u64) -> DayAgg {
    let mut agg = DayAgg::new();
    for s in stats {
        if s.time >= min_time { agg.add(s); }
    }
    agg
}

fn render_analytics_table(f: &mut Frame, area: Rect, stats: &[BlockStats], missing: u64, scroll: u16) {
    let mut daily: BTreeMap<String, DayAgg> = BTreeMap::new();
    for s in stats {
        let date = chrono::DateTime::from_timestamp(s.time as i64, 0)
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        daily.entry(date).or_insert_with(DayAgg::new).add(s);
    }

    let rows: Vec<Row> = daily.iter().rev().map(|(date, d)| analytics_data_row(date, d)).collect();

    let block_count = stats.len();
    let table = Table::new(rows, analytics_widths())
        .header(analytics_header_row())
        .block(
            Block::default()
                .borders(Borders::ALL).border_type(BorderType::Rounded)
                .title(if missing > 0 {
                    format!(" Daily Breakdown — {} blocks ({} missing) ", block_count, missing)
                } else {
                    format!(" Daily Breakdown — {} blocks ", block_count)
                })
                .style(Style::default().fg(Color::Cyan)),
        )
        .row_highlight_style(Style::default());

    let mut tstate = TableState::default().with_offset(scroll as usize);
    f.render_stateful_widget(table, area, &mut tstate);
}
