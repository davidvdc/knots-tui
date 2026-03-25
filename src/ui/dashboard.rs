use crate::rpc::{BlockInfo, BlockStats, NodeData};
use crate::service::AppService;
use crate::sys::SystemStats;
use crate::AnalyticsData;
use crossterm::event::KeyCode;
use ratatui::{prelude::*, widgets::*};
use std::collections::HashMap;
use std::sync::Arc;

use super::analytics::{aggregate_period, analytics_data_row, analytics_header_row, analytics_widths};
use super::common::*;
use super::{KeyResult, Screen, StateRef};

pub struct DashboardScreen {
    svc: Arc<AppService>,
    state: StateRef,
    blocks_focused: bool,
    selected_block: u16,
    block_scroll: u16,
    peer_scroll: u16,
    show_block_modal: bool,
    show_warnings_modal: bool,
}

impl DashboardScreen {
    pub fn new(svc: Arc<AppService>, state: StateRef) -> Self {
        Self { svc, state, blocks_focused: true, selected_block: 0, block_scroll: 0, peer_scroll: 0, show_block_modal: false, show_warnings_modal: false }
    }
}

impl Screen for DashboardScreen {
    fn name(&self) -> &str { "Dashboard" }

    fn footer_hint(&self) -> &str {
        let has_warnings = !self.state.borrow().node_data.blockchain.warnings.as_str().is_empty();
        if has_warnings {
            " q: quit | Tab: switch screen | j/k: switch table | ↑/↓: navigate | r: refresh | F1: warnings "
        } else {
            " q: quit | Tab: switch screen | j/k: switch table | ↑/↓: navigate | r: refresh "
        }
    }

    fn draw(&self, f: &mut Frame, area: Rect) {
        let state = self.state.borrow();
        let data = &state.node_data;

        if let Some(ref err) = data.error {
            let err_block = Block::default()
                .borders(Borders::ALL).border_type(BorderType::Rounded)
                .title(" Error ").style(Style::default().fg(Color::Red));
            f.render_widget(Paragraph::new(err.clone()).block(err_block).wrap(Wrap { trim: true }).style(Style::default().fg(Color::Red)), area);
            return;
        }

        if data.network.subversion.is_empty() {
            let block = Block::default()
                .borders(Borders::ALL).border_type(BorderType::Rounded)
                .title(" Dashboard ").style(Style::default().fg(Color::Cyan));
            let text = Paragraph::new(vec![
                Line::from(""), Line::from(""),
                Line::from(Span::styled("Connecting to Bitcoin Knots node...", Style::default().fg(Color::Yellow))),
                Line::from(""),
                Line::from(Span::styled("Waiting for first RPC response", Style::default().fg(Color::DarkGray))),
            ]).block(block).alignment(Alignment::Center);
            f.render_widget(text, area);
            return;
        }

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(9), Constraint::Min(8)])
            .split(area);

        let top_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(25), Constraint::Percentage(25), Constraint::Percentage(25), Constraint::Percentage(25)])
            .split(rows[0]);

        draw_blockchain_card(f, top_cols[0], data);
        draw_mempool_card(f, top_cols[1], data);
        draw_network_card(f, top_cols[2], data);
        let uses_tor = data.peers.iter().any(|p| p.addr.contains(".onion"))
            || data.network.localaddresses.iter().any(|a| a.address.contains(".onion"));
        draw_system_card(f, top_cols[3], &state.system_stats, uses_tor);

        let bottom_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(11), Constraint::Length(4), Constraint::Min(8)])
            .split(rows[1]);

        draw_blocks_table(f, bottom_rows[0], data, &state.block_stats_cache, self.selected_block, self.block_scroll, self.blocks_focused);
        draw_analytics_summary(f, bottom_rows[1], &state.analytics);
        draw_peers_table(f, bottom_rows[2], data, self.peer_scroll, !self.blocks_focused);
    }

    fn handle_key(&mut self, key: KeyCode) -> KeyResult {
        let state = self.state.borrow();
        match key {
            KeyCode::Down => {
                if self.blocks_focused {
                    let max = state.node_data.recent_blocks.len().saturating_sub(1) as u16;
                    if self.selected_block == max && !self.svc.is_fetching_older_blocks() {
                        if let Some(lowest) = state.node_data.recent_blocks.last().map(|b| b.height) {
                            if lowest > 1 {
                                let end = lowest.saturating_sub(1);
                                let start = end.saturating_sub(49);
                                self.svc.fetch_older_blocks(start, end);
                            }
                        }
                    }
                    self.selected_block = (self.selected_block + 1).min(max);
                    if self.selected_block >= self.block_scroll + 8 { self.block_scroll = self.selected_block - 7; }
                } else {
                    self.peer_scroll = self.peer_scroll.saturating_add(1);
                }
                KeyResult::None
            }
            KeyCode::Up => {
                if self.blocks_focused {
                    self.selected_block = self.selected_block.saturating_sub(1);
                    if self.selected_block < self.block_scroll { self.block_scroll = self.selected_block; }
                } else {
                    self.peer_scroll = self.peer_scroll.saturating_sub(1);
                }
                KeyResult::None
            }
            KeyCode::Char('j') | KeyCode::Char('k') => { self.blocks_focused = !self.blocks_focused; KeyResult::None }
            KeyCode::Enter => {
                let max = state.node_data.recent_blocks.len().saturating_sub(1) as u16;
                if self.selected_block <= max {
                    if let Some(b) = state.node_data.recent_blocks.get(self.selected_block as usize) {
                        if state.block_stats_cache.contains_key(&b.height) { self.show_block_modal = true; }
                    }
                }
                KeyResult::None
            }
            KeyCode::Char('r') => { self.svc.force_refresh(); KeyResult::None }
            KeyCode::F(1) => {
                if !self.state.borrow().node_data.blockchain.warnings.as_str().is_empty() {
                    self.show_warnings_modal = true;
                }
                KeyResult::None
            }
            KeyCode::Esc => KeyResult::Quit,
            _ => KeyResult::None,
        }
    }

    fn has_modal(&self) -> bool { self.show_block_modal || self.show_warnings_modal }

    fn draw_modal(&self, f: &mut Frame, area: Rect) {
        if self.show_warnings_modal {
            let state = self.state.borrow();
            draw_warnings_modal(f, area, &state.node_data.blockchain.warnings.as_str());
            return;
        }
        let state = self.state.borrow();
        if let Some(b) = state.node_data.recent_blocks.get(self.selected_block as usize) {
            if let Some(s) = state.block_stats_cache.get(&b.height) {
                draw_block_modal(f, area, b, s);
            }
        }
    }

    fn handle_modal_key(&mut self, key: KeyCode) {
        if self.show_warnings_modal {
            match key {
                KeyCode::Esc | KeyCode::F(1) | KeyCode::Char('q') => { self.show_warnings_modal = false; }
                _ => {}
            }
            return;
        }
        let state = self.state.borrow();
        match key {
            KeyCode::Esc | KeyCode::Char('q') => { self.show_block_modal = false; }
            KeyCode::Down => {
                let max = state.node_data.recent_blocks.len().saturating_sub(1) as u16;
                if self.selected_block == max && !self.svc.is_fetching_older_blocks() {
                    if let Some(lowest) = state.node_data.recent_blocks.last().map(|b| b.height) {
                        if lowest > 1 {
                            let end = lowest.saturating_sub(1);
                            let start = end.saturating_sub(49);
                            self.svc.fetch_older_blocks(start, end);
                        }
                    }
                }
                self.selected_block = (self.selected_block + 1).min(max);
                if self.selected_block >= self.block_scroll + 8 { self.block_scroll = self.selected_block - 7; }
            }
            KeyCode::Up => {
                self.selected_block = self.selected_block.saturating_sub(1);
                if self.selected_block < self.block_scroll { self.block_scroll = self.selected_block; }
            }
            _ => {}
        }
    }

    fn available(&self) -> bool {
        !self.state.borrow().node_data.blockchain.initialblockdownload
    }

    fn on_enter(&mut self) {
        self.svc.set_loading(true);
        self.svc.force_refresh();
    }
}

// --- Shared component used by IBD screen ---

pub fn draw_peers_table(f: &mut Frame, area: Rect, data: &NodeData, scroll: u16, focused: bool) {
    let header = Row::new(vec!["ID", "Address", "Client", "Type", "TX", "Dir", "Height", "Ping", "Conn", "Sent", "Recv"])
        .style(Style::default().fg(Color::Cyan).bold()).bottom_margin(0);
    let now = data.fetched_at;
    let rows: Vec<Row> = data.peers.iter().map(|p| {
        let dir = if p.inbound { "in" } else { "out" };
        let ping = p.pingtime.map(|t| format!("{:.0}ms", t * 1000.0)).unwrap_or_else(|| "-".to_string());
        let client = p.subver.trim_matches('/').to_string();
        let uptime = if p.conntime > 0 && now > p.conntime { format_duration(now - p.conntime) } else { "-".to_string() };
        let relay = if p.relaytxes { "yes" } else { "no" };
        Row::new(vec![p.id.to_string(), p.addr.clone(), client, p.connection_type.clone(), relay.to_string(), dir.to_string(), p.synced_blocks.to_string(), ping, uptime, format_bytes_short(p.bytessent), format_bytes_short(p.bytesrecv)])
            .style(Style::default().fg(if p.inbound { Color::White } else { Color::Gray }))
    }).collect();
    let widths = [Constraint::Length(4), Constraint::Min(18), Constraint::Min(30), Constraint::Length(19), Constraint::Length(3), Constraint::Length(4), Constraint::Length(8), Constraint::Length(7), Constraint::Length(12), Constraint::Length(8), Constraint::Length(8)];
    let border_color = if focused { Color::Yellow } else { Color::default() };
    let table = Table::new(rows, widths).header(header).block(
        Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color))
            .title(format!(" Peers ({}) | known: {} ", data.peers.len(), format_number(data.known_peers)))
            .title_style(Style::default().fg(Color::Cyan).bold()))
        .row_highlight_style(Style::default().bg(Color::DarkGray));
    let mut tstate = TableState::default().with_offset(scroll as usize);
    f.render_stateful_widget(table, area, &mut tstate);
}

// --- Private rendering helpers (unchanged) ---

fn draw_block_modal(f: &mut Frame, area: Rect, block: &BlockInfo, stats: &BlockStats) {
    let modal_width = (area.width as f32 * 0.75) as u16;
    let modal_height = 48u16.min(area.height - 4);
    let x = (area.width.saturating_sub(modal_width)) / 2;
    let y = (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(x, y, modal_width, modal_height);
    let dim = Block::default().style(Style::default().bg(Color::Black));
    f.render_widget(Clear, modal_area); f.render_widget(dim, area);

    let user_tx = stats.tx_count.saturating_sub(1);
    let pct = |count: usize| -> f64 { if user_tx > 0 { (count as f64 / user_tx as f64) * 100.0 } else { 0.0 } };
    let wpct = |vs: u64| -> f64 { if stats.total_vsize > 0 { (vs as f64 / stats.total_vsize as f64) * 100.0 } else { 0.0 } };
    let viol_color = |count: usize| -> Color { if count > 0 { Color::Red } else { Color::Green } };

    // Protocol table data: (label, count, vsize, bip110_violations, matrix_idx)
    let protocols: Vec<(&str, usize, u64, usize, usize)> = vec![
        ("Financial",    stats.financial_count,      stats.financial_vsize,      stats.financial_bip110v,      0),
        ("Runes",        stats.rune_count,           stats.rune_vsize,           stats.rune_bip110v,           1),
        ("Inscriptions", stats.inscription_count,    stats.inscription_vsize,    stats.inscription_bip110v,    3),
        ("BRC-20",       stats.brc20_count,          stats.brc20_vsize,          stats.brc20_bip110v,          2),
        ("OPNET",        stats.opnet_count,          stats.opnet_vsize,          stats.opnet_bip110v,          4),
        ("Stamps",       stats.stamp_count,          stats.stamp_vsize,          stats.stamp_bip110v,          5),
        ("Counterparty", stats.counterparty_count,   stats.counterparty_vsize,   stats.counterparty_bip110v,   6),
        ("Omni",         stats.omni_count,           stats.omni_vsize,           stats.omni_bip110v,           7),
        ("OP_RET other", stats.opreturn_other_count, stats.opreturn_other_vsize, stats.opreturn_other_bip110v, 8),
        ("Other data",   stats.other_data_count,     stats.other_data_vsize,     0,                            9),
    ];

    let hdr = Style::default().fg(Color::Cyan).bold();
    let rhdr = Style::default().fg(Color::Red).bold();
    let sep = Style::default().fg(Color::DarkGray);
    let table_header = Line::from(vec![
        Span::styled(format!("{:<14}", ""), hdr),
        Span::styled(format!("{:>6}", "Count"), hdr),
        Span::styled(format!("{:>7}", "%"), hdr),
        Span::styled(format!("{:>7}", "Weight"), hdr),
        Span::styled(format!("{:>7}", "Wt%"), hdr),
        Span::styled(" | ", sep),
        Span::styled(format!("{:>5}", "R1"), rhdr),
        Span::styled(format!("{:>5}", "R2"), rhdr),
        Span::styled(format!("{:>5}", "R3"), rhdr),
        Span::styled(format!("{:>5}", "R4"), rhdr),
        Span::styled(format!("{:>5}", "R5"), rhdr),
        Span::styled(format!("{:>5}", "R6"), rhdr),
        Span::styled(format!("{:>5}", "R7"), rhdr),
    ]);

    let mut table_rows: Vec<Line> = Vec::new();
    for (label, count, vsize, _violations, mi) in &protocols {
        let is_fin = *label == "Financial";
        if !is_fin && *count == 0 {
            // Show non-financial rows even at zero count, but dimmed
            table_rows.push(Line::from(vec![Span::styled(format!("  {:<12}", label), Style::default().fg(Color::DarkGray))]));
            continue;
        }
        if is_fin && *count == 0 { continue; }
        let row_color = if is_fin { Color::Green } else { Color::Yellow };
        let rules = &stats.bip110_rule_matrix[*mi];
        let mut spans = vec![
            Span::styled(format!("  {:<12}", label), Style::default().fg(row_color)),
            Span::styled(format!("{:>6}", count), Style::default().fg(Color::White)),
            Span::styled(format!("{:>6.1}%", pct(*count)), Style::default().fg(row_color)),
            Span::styled(format!("{:>7}", format_bytes_short(*vsize)), Style::default().fg(Color::White)),
            Span::styled(format!("{:>6.1}%", wpct(*vsize)), Style::default().fg(row_color)),
            Span::styled(" | ", sep),
        ];
        for &rv in rules {
            let s = if rv > 0 { format!("{}", rv) } else { String::new() };
            let c = if rv > 0 { Color::Red } else { Color::DarkGray };
            spans.push(Span::styled(format!("{:>5}", s), Style::default().fg(c)));
        }
        table_rows.push(Line::from(spans));
    }

    let taproot_spend_pct = pct(stats.taproot_spend_count);
    let taproot_output_pct = pct(stats.taproot_output_count);

    let mut text = vec![
        Line::from(vec![
            Span::styled("Output: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format_btc(stats.total_out), Style::default().fg(Color::White).bold()),
            Span::styled("   Fees: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format_btc_fees(stats.total_fee), Style::default().fg(Color::White).bold()),
            Span::styled("   TXs: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{}", user_tx), Style::default().fg(Color::White).bold()),
        ]),
        Line::from(vec![
            Span::styled("Weight: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{} WU ({:.1}%)", format_number(block.weight), block.weight as f64 / 4_000_000.0 * 100.0), Style::default().fg(Color::White).bold()),
            Span::styled("   Taproot: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("spend {} ({:.1}%)", stats.taproot_spend_count, taproot_spend_pct), Style::default().fg(Color::White)),
            Span::styled(format!("  out {} ({:.1}%)", stats.taproot_output_count, taproot_output_pct), Style::default().fg(Color::White)),
        ]),
        Line::from(""),
        table_header,
    ];
    text.extend(table_rows);

    let gray = Style::default().fg(Color::DarkGray);
    text.push(Line::from(""));

    // Protocol descriptions — one per line
    text.extend_from_slice(&[
        Line::from(vec![Span::styled("  Runes         ", Style::default().fg(Color::Yellow)), Span::styled("Fungible tokens via OP_RETURN (OP_13 tag)", gray)]),
        Line::from(vec![Span::styled("  Inscriptions  ", Style::default().fg(Color::Yellow)), Span::styled("Ordinals data embedded in witness (images, text, etc.)", gray)]),
        Line::from(vec![Span::styled("  BRC-20        ", Style::default().fg(Color::Yellow)), Span::styled("Token standard via ordinals inscription envelopes", gray)]),
        Line::from(vec![Span::styled("  OPNET         ", Style::default().fg(Color::Yellow)), Span::styled("Smart contracts via tapscript execution", gray)]),
        Line::from(vec![Span::styled("  Stamps        ", Style::default().fg(Color::Yellow)), Span::styled("SRC-20 tokens encoded in bare multisig outputs", gray)]),
        Line::from(vec![Span::styled("  Counterparty  ", Style::default().fg(Color::Yellow)), Span::styled("XCP asset protocol via OP_RETURN (CNTRPRTY)", gray)]),
        Line::from(vec![Span::styled("  Omni          ", Style::default().fg(Color::Yellow)), Span::styled("Token layer via OP_RETURN (ex-Mastercoin)", gray)]),
        Line::from(""),
        Line::from(vec![Span::styled("  R1 ", Style::default().fg(Color::Red)), Span::styled("OP_RETURN >83 bytes or scriptPubKey >34 bytes", gray)]),
        Line::from(vec![Span::styled("  R2 ", Style::default().fg(Color::Red)), Span::styled("Witness element >256 bytes", gray)]),
        Line::from(vec![Span::styled("  R3 ", Style::default().fg(Color::Red)), Span::styled("Spending undefined witness or tapleaf version", gray)]),
        Line::from(vec![Span::styled("  R4 ", Style::default().fg(Color::Red)), Span::styled("Witness stack contains taproot annex", gray)]),
        Line::from(vec![Span::styled("  R5 ", Style::default().fg(Color::Red)), Span::styled("Taproot control block >257 bytes", gray)]),
        Line::from(vec![Span::styled("  R6 ", Style::default().fg(Color::Red)), Span::styled("Tapscript contains OP_SUCCESS opcode", gray)]),
        Line::from(vec![Span::styled("  R7 ", Style::default().fg(Color::Red)), Span::styled("Tapscript executes OP_IF or OP_NOTIF", gray)]),
        Line::from(""),
    ]);

    // BIP-110 summary
    let compl_txs = user_tx.saturating_sub(stats.bip110_violating_txs);
    let compl_pct = if user_tx > 0 { compl_txs as f64 / user_tx as f64 * 100.0 } else { 100.0 };
    let savings_pct = wpct(stats.bip110_violating_vsize);
    text.push(Line::from(vec![
        Span::styled("BIP-110: ", Style::default().fg(Color::Cyan).bold()),
        Span::styled(format!("{:.1}% compliant", compl_pct), Style::default().fg(if stats.bip110_violating_txs == 0 { Color::Green } else { Color::Yellow }).bold()),
        Span::styled(format!("  ({} violating, {:.1}% weight savings)", stats.bip110_violating_txs, savings_pct), gray),
    ]));

    // Max observed sizes — one per line with full descriptions
    if stats.max_opreturn_size > 0 {
        text.push(Line::from(vec![
            Span::styled("  Largest OP_RETURN:       ", gray),
            Span::styled(format!("{} bytes", stats.max_opreturn_size), Style::default().fg(if stats.max_opreturn_size > 83 { Color::Red } else { Color::Green })),
            Span::styled(format!("  (limit: 83 bytes)"), gray),
        ]));
    }
    if stats.max_spk_size > 0 {
        text.push(Line::from(vec![
            Span::styled("  Largest scriptPubKey:    ", gray),
            Span::styled(format!("{} bytes", stats.max_spk_size), Style::default().fg(if stats.max_spk_size > 34 { Color::Red } else { Color::Green })),
            Span::styled(format!("  (limit: 34 bytes)"), gray),
        ]));
    }
    if stats.max_witness_item_size > 0 {
        text.push(Line::from(vec![
            Span::styled("  Largest witness element: ", gray),
            Span::styled(format!("{} bytes", stats.max_witness_item_size), Style::default().fg(if stats.max_witness_item_size > 256 { Color::Red } else { Color::Green })),
            Span::styled(format!("  (limit: 256 bytes)"), gray),
        ]));
    }

    text.push(Line::from(""));
    text.push(Line::from(Span::styled("↑/↓: prev/next block | Esc: close", gray)));

    let title = format!(" Block {} ", format_number(block.height));
    let modal_block = Block::default().borders(Borders::ALL).border_type(BorderType::Double)
        .title(title).title_style(Style::default().fg(Color::Cyan).bold()).style(Style::default().bg(Color::Black));
    f.render_widget(Clear, modal_area);
    f.render_widget(Paragraph::new(text).block(modal_block).wrap(Wrap { trim: false }), modal_area);
}

fn draw_blockchain_card(f: &mut Frame, area: Rect, data: &NodeData) {
    let bc = &data.blockchain; let progress = bc.verificationprogress * 100.0;
    let synced = if progress >= 99.99 { "YES" } else { "syncing" };
    let disk_str = if bc.pruned {
        format!("{} (pruned)", format_bytes(bc.size_on_disk))
    } else {
        format_bytes(bc.size_on_disk)
    };
    let lines = vec![
        Line::from(vec![Span::styled("Height:  ", Style::default().fg(Color::DarkGray)), Span::styled(format_number(bc.blocks), Style::default().fg(Color::Yellow).bold())]),
        Line::from(vec![Span::styled("Headers: ", Style::default().fg(Color::DarkGray)), Span::styled(format_number(bc.headers), Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled("Synced:  ", Style::default().fg(Color::DarkGray)), Span::styled(format!("{} ({:.2}%)", synced, progress), Style::default().fg(if progress >= 99.99 { Color::Green } else { Color::Yellow }))]),
        Line::from(vec![Span::styled("Diff:    ", Style::default().fg(Color::DarkGray)), Span::styled(format!("{:.2e}", bc.difficulty), Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled("Hashrate:", Style::default().fg(Color::DarkGray)), Span::styled(format!(" {}", format_hashrate(data.mining.networkhashps)), Style::default().fg(Color::Yellow))]),
        Line::from(vec![Span::styled("Disk:    ", Style::default().fg(Color::DarkGray)), Span::styled(disk_str, Style::default().fg(Color::White))]),
    ];
    f.render_widget(Paragraph::new(lines).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" Blockchain ").title_style(Style::default().fg(Color::Cyan).bold())), area);
}

fn draw_mempool_card(f: &mut Frame, area: Rect, data: &NodeData) {
    let mp = &data.mempool;
    let lines = vec![
        Line::from(vec![Span::styled("TXs:      ", Style::default().fg(Color::DarkGray)), Span::styled(format_number(mp.size), Style::default().fg(Color::Yellow).bold())]),
        Line::from(vec![Span::styled("Size:     ", Style::default().fg(Color::DarkGray)), Span::styled(format_bytes(mp.bytes), Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled("Memory:   ", Style::default().fg(Color::DarkGray)), Span::styled(format!("{} / {}", format_bytes(mp.usage), format_bytes(mp.maxmempool)), Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled("Fees:     ", Style::default().fg(Color::DarkGray)), Span::styled(format!("{:.8} BTC", mp.total_fee), Style::default().fg(Color::Green))]),
        Line::from(vec![Span::styled("Min fee:  ", Style::default().fg(Color::DarkGray)), Span::styled(format!("{:.8} BTC/kvB", mp.mempoolminfee), Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled("Relay fee:", Style::default().fg(Color::DarkGray)), Span::styled(format!(" {:.8} BTC/kvB", mp.minrelaytxfee), Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled("Loaded:   ", Style::default().fg(Color::DarkGray)), Span::styled(if mp.loaded { "yes" } else { "no" }.to_string(), Style::default().fg(if mp.loaded { Color::Green } else { Color::Yellow }))]),
    ];
    f.render_widget(Paragraph::new(lines).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" Mempool ").title_style(Style::default().fg(Color::Magenta).bold())), area);
}

fn draw_network_card(f: &mut Frame, area: Rect, data: &NodeData) {
    let net = &data.network; let nt = &data.net_totals;
    let gray = Style::default().fg(Color::DarkGray);
    let mut lines = vec![
        Line::from(vec![Span::styled("Conns:    ", gray), Span::styled(format!("{} (in: {} / out: {})", net.connections, net.connections_in, net.connections_out), Style::default().fg(Color::Yellow).bold())]),
        Line::from(vec![Span::styled("Protocol: ", gray), Span::styled(format!("{}", net.protocolversion), Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled("Recv:     ", gray), Span::styled(format_bytes_short(nt.totalbytesrecv), Style::default().fg(Color::Green))]),
        Line::from(vec![Span::styled("Sent:     ", gray), Span::styled(format_bytes_short(nt.totalbytessent), Style::default().fg(Color::Red))]),
        Line::from(vec![Span::styled("Relay fee:", gray), Span::styled(format!(" {:.8} BTC/kvB", net.relayfee), Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled("Incr fee: ", gray), Span::styled(format!("{:.8} BTC/kvB", net.incrementalfee), Style::default().fg(Color::White))]),
    ];
    if !net.localaddresses.is_empty() {
        let addrs: Vec<String> = net.localaddresses.iter().map(|a| format!("{}:{}", a.address, a.port)).collect();
        lines.push(Line::from(vec![Span::styled("Local:    ", gray), Span::styled(addrs.join(", "), Style::default().fg(Color::White))]));
    }
    f.render_widget(Paragraph::new(lines).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" Network ").title_style(Style::default().fg(Color::Green).bold())), area);
}

fn draw_system_card(f: &mut Frame, area: Rect, sys: &SystemStats, uses_tor: bool) {
    let gray = Style::default().fg(Color::DarkGray);
    let white = Style::default().fg(Color::White);
    let show_tor = uses_tor && sys.tor.found;
    let show_btc = sys.bitcoind.found;

    let cpu_total = if !sys.cpus.is_empty() {
        let sum: f32 = sys.cpus.iter().map(|c| c.user_pct + c.system_pct + c.nice_pct + c.iowait_pct).sum();
        sum / sys.cpus.len() as f32
    } else { 0.0 };
    let mem_used = sys.mem.used;

    let pct_color = |pct: f32, hi: f32, mid: f32| -> Color {
        if pct > hi { Color::Red } else if pct > mid { Color::Yellow } else { Color::Green }
    };

    // Column width for values
    let w = 6usize;

    // Header
    let mut hdr = vec![Span::styled(format!("{:>4}", ""), gray), Span::styled(format!("{:>w$}", "Sys"), Style::default().fg(Color::Cyan))];
    if show_btc { hdr.push(Span::styled(format!("{:>w$}", "Btc"), Style::default().fg(Color::Cyan))); }
    if show_tor { hdr.push(Span::styled(format!("{:>w$}", "Tor"), Style::default().fg(Color::Magenta))); }
    hdr.push(Span::styled(format!("{:>w$}", "Of"), gray));

    // CPU row
    let mut cpu_row = vec![
        Span::styled("CPU ", gray),
        Span::styled(format!("{:>w$}", format!("{:.0}%", cpu_total)), Style::default().fg(pct_color(cpu_total, 80.0, 50.0))),
    ];
    if show_btc { cpu_row.push(Span::styled(format!("{:>w$}", format!("{:.0}%", sys.bitcoind.cpu_pct)), Style::default().fg(pct_color(sys.bitcoind.cpu_pct, 80.0, 30.0)))); }
    if show_tor { cpu_row.push(Span::styled(format!("{:>w$}", format!("{:.0}%", sys.tor.cpu_pct)), Style::default().fg(pct_color(sys.tor.cpu_pct, 50.0, 10.0)))); }
    cpu_row.push(Span::styled(format!("{:>w$}", format!("{}c", sys.cpus.len())), gray));

    // Mem row
    let mut mem_row = vec![
        Span::styled("Mem ", gray),
        Span::styled(format!("{:>w$}", format_bytes_short(mem_used)), white),
    ];
    if show_btc { mem_row.push(Span::styled(format!("{:>w$}", format_bytes_short(sys.bitcoind.rss)), white)); }
    if show_tor { mem_row.push(Span::styled(format!("{:>w$}", format_bytes_short(sys.tor.rss)), white)); }
    mem_row.push(Span::styled(format!("{:>w$}", format_bytes_short(sys.mem.total)), gray));

    // Swap row (only if swap exists)
    let mut lines = vec![Line::from(hdr), Line::from(cpu_row), Line::from(mem_row)];

    if sys.mem.swap_total > 0 && sys.mem.swap_used > 0 {
        let swap_pct = sys.mem.swap_used as f32 / sys.mem.swap_total as f32 * 100.0;
        let mut swap_row = vec![
            Span::styled("Swp ", gray),
            Span::styled(format!("{:>w$}", format_bytes_short(sys.mem.swap_used)), Style::default().fg(pct_color(swap_pct, 50.0, 10.0))),
        ];
        // Pad btc/tor columns
        if show_btc { swap_row.push(Span::raw(format!("{:>w$}", ""))); }
        if show_tor { swap_row.push(Span::raw(format!("{:>w$}", ""))); }
        swap_row.push(Span::styled(format!("{:>w$}", format_bytes_short(sys.mem.swap_total)), gray));
        lines.push(Line::from(swap_row));
    }

    // Disk IO
    let disks: Vec<&crate::sys::DiskIO> = sys.disks.iter().take(4).collect();
    if !disks.is_empty() {
        lines.push(Line::from(""));
        let dw = 6usize;
        let mut hdr = vec![Span::styled("    ", gray)];
        for d in &disks { hdr.push(Span::styled(format!("{:>dw$}", d.name), Style::default().fg(Color::Cyan))); }
        lines.push(Line::from(hdr));

        let mut r_row = vec![Span::styled("IO R", gray)];
        for d in &disks { r_row.push(Span::styled(format!("{:>dw$}", format_bytes_short(d.read_per_sec).trim_start()), Style::default().fg(Color::Green))); }
        lines.push(Line::from(r_row));

        let mut w_row = vec![Span::styled("IO W", gray)];
        for d in &disks { w_row.push(Span::styled(format!("{:>dw$}", format_bytes_short(d.write_per_sec).trim_start()), Style::default().fg(Color::Yellow))); }
        lines.push(Line::from(w_row));
    }

    f.render_widget(Paragraph::new(lines).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" System ").title_style(Style::default().fg(Color::Yellow).bold())), area);
}

fn draw_warnings_modal(f: &mut Frame, area: Rect, warnings: &str) {
    let modal_width = (area.width as f32 * 0.6) as u16;
    let modal_height = 12u16.min(area.height - 4);
    let x = (area.width.saturating_sub(modal_width)) / 2;
    let y = (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(x, y, modal_width, modal_height);

    f.render_widget(Clear, modal_area);
    f.render_widget(Block::default().style(Style::default().bg(Color::Black)), area);

    let mut lines = vec![Line::from("")];
    for line in warnings.lines() {
        lines.push(Line::from(Span::styled(line.to_string(), Style::default().fg(Color::Red))));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Press Esc or F1 to close", Style::default().fg(Color::DarkGray))));

    let block = Block::default().borders(Borders::ALL).border_type(BorderType::Double)
        .title(" Warnings ").title_style(Style::default().fg(Color::Red).bold())
        .style(Style::default().bg(Color::Black));
    f.render_widget(Clear, modal_area);
    f.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: false }), modal_area);
}

fn draw_analytics_summary(f: &mut Frame, area: Rect, analytics: &AnalyticsData) {
    let now = chrono::Utc::now().timestamp() as u64; let cutoff = now.saturating_sub(86400);
    let agg = aggregate_period(&analytics.stats, cutoff);
    let block = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" Analytics ").style(Style::default().fg(Color::Cyan));
    if agg.blocks > 0 {
        f.render_widget(Table::new(vec![analytics_data_row("24h", &agg)], analytics_widths()).header(analytics_header_row()).block(block), area);
    } else {
        f.render_widget(Paragraph::new(Line::from(Span::styled("  Waiting for block analysis data...", Style::default().fg(Color::DarkGray)))).block(block), area);
    }
}

fn draw_blocks_table(f: &mut Frame, area: Rect, data: &NodeData, block_stats: &HashMap<u64, BlockStats>, selected_block: u16, block_scroll: u16, focused: bool) {
    let header = Row::new(vec![Cell::from(" "), Cell::from("Height"), Cell::from("Time"), Cell::from("TXs"), Cell::from("Size"), Cell::from("Weight"), Cell::from("Age"), Cell::from("BIP110"), Cell::from("BTC Out"), Cell::from("Fees"), Cell::from("Fin%"), Cell::from(Line::from("!110").alignment(Alignment::Right)), Cell::from("%")])
        .style(Style::default().fg(Color::Cyan).bold()).bottom_margin(0);
    let now = chrono::Utc::now().timestamp() as u64; let scroll = block_scroll as usize;
    let visible_blocks: Vec<(usize, &BlockInfo)> = data.recent_blocks.iter().enumerate().skip(scroll).take(8).collect();
    let rows: Vec<Row> = visible_blocks.iter().map(|&(i, b)| {
        let age = if b.time > 0 && now > b.time { format_duration(now - b.time) } else { "-".to_string() };
        let bip110 = if b.version >= 0x20000000 && b.version & (1 << 4) != 0 { "yes" } else { "no" };
        let bip110_color = if bip110 == "yes" { Color::Green } else { Color::DarkGray };
        let marker = if focused && i == selected_block as usize { ">" } else { " " };
        let (btc_out, fees, financial, fin_color, viol_count, viol_pct_str, viol_color) = if let Some(s) = block_stats.get(&b.height) {
            let user_tx = s.tx_count.saturating_sub(1);
            let pct = if user_tx > 0 { (s.financial_count as f64 / user_tx as f64) * 100.0 } else { 100.0 };
            let color = if pct >= 90.0 { Color::Green } else if pct >= 70.0 { Color::Yellow } else { Color::Red };
            let viol_pct = if user_tx > 0 { s.bip110_violating_txs as f64 / user_tx as f64 * 100.0 } else { 0.0 };
            let vc = if s.bip110_violating_txs == 0 { Color::Green } else if viol_pct <= 1.0 { Color::Yellow } else { Color::Red };
            (format_btc(s.total_out), format_btc_fees(s.total_fee), format!("{:.0}%", pct), color, format!("{}", s.bip110_violating_txs), if s.bip110_violating_txs > 0 { format!("{:.1}%", viol_pct) } else { String::new() }, vc)
        } else { ("-".into(), "-".into(), "-".into(), Color::DarkGray, "-".into(), String::new(), Color::DarkGray) };
        let timestamp = if b.time > 0 { chrono::DateTime::from_timestamp(b.time as i64, 0).map(|dt| dt.format("%m-%d %H:%M").to_string()).unwrap_or("-".into()) } else { "-".into() };
        Row::new(vec![
            Cell::from(Span::styled(marker, Style::default().fg(Color::Yellow))), Cell::from(format_number(b.height)), Cell::from(timestamp), Cell::from(format_number(b.tx_count as u64)), Cell::from(format_bytes_short(b.size)), Cell::from(format!("{:>7.1} kvWU", b.weight as f64 / 1000.0)), Cell::from(age), Cell::from(Span::styled(bip110.to_string(), Style::default().fg(bip110_color))), Cell::from(btc_out), Cell::from(fees), Cell::from(Span::styled(financial, Style::default().fg(fin_color))), Cell::from(Line::from(Span::styled(viol_count, Style::default().fg(viol_color))).alignment(Alignment::Right)), Cell::from(Span::styled(viol_pct_str, Style::default().fg(viol_color))),
        ])
    }).collect();
    let widths = vec![Constraint::Length(2), Constraint::Length(10), Constraint::Length(12), Constraint::Length(7), Constraint::Length(10), Constraint::Length(12), Constraint::Length(12), Constraint::Length(7), Constraint::Length(12), Constraint::Length(12), Constraint::Length(5), Constraint::Length(5), Constraint::Min(5)];
    let total = data.recent_blocks.len();
    let title = format!(" Recent Blocks ({}-{}/{}) [Enter: detail] ", scroll + 1, (scroll + 8).min(total), total);
    let border_color = if focused { Color::Yellow } else { Color::default() };
    f.render_widget(Table::new(rows, widths).header(header).block(
        Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(border_color)).title(title).title_style(Style::default().fg(Color::Yellow).bold())), area);
}
