use crate::rpc::{BlockInfo, BlockStats, NodeData};
use crate::service::AppService;
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
}

impl DashboardScreen {
    pub fn new(svc: Arc<AppService>, state: StateRef) -> Self {
        Self { svc, state, blocks_focused: true, selected_block: 0, block_scroll: 0, peer_scroll: 0, show_block_modal: false }
    }
}

impl Screen for DashboardScreen {
    fn name(&self) -> &str { "Dashboard" }

    fn footer_hint(&self) -> &str {
        " q: quit | Tab: switch screen | j/k: switch table | ↑/↓: navigate | r: refresh "
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
        draw_mining_card(f, top_cols[3], data);

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
            KeyCode::Esc => KeyResult::Quit,
            _ => KeyResult::None,
        }
    }

    fn has_modal(&self) -> bool { self.show_block_modal }

    fn draw_modal(&self, f: &mut Frame, area: Rect) {
        let state = self.state.borrow();
        if let Some(b) = state.node_data.recent_blocks.get(self.selected_block as usize) {
            if let Some(s) = state.block_stats_cache.get(&b.height) {
                draw_block_modal(f, area, b, s);
            }
        }
    }

    fn handle_modal_key(&mut self, key: KeyCode) {
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
    let modal_width = (area.width as f32 * 0.65) as u16;
    let modal_height = 43u16.min(area.height - 4);
    let x = (area.width.saturating_sub(modal_width)) / 2;
    let y = (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(x, y, modal_width, modal_height);
    let dim = Block::default().style(Style::default().bg(Color::Black));
    f.render_widget(Clear, modal_area); f.render_widget(dim, area);

    let user_tx = stats.tx_count.saturating_sub(1);
    let data_count = user_tx.saturating_sub(stats.financial_count);
    let data_vsize = stats.total_vsize.saturating_sub(stats.financial_vsize);
    let pct = |count: usize| -> f64 { if user_tx > 0 { (count as f64 / user_tx as f64) * 100.0 } else { 0.0 } };
    let fin_pct = pct(stats.financial_count); let data_pct = pct(data_count);
    let taproot_spend_pct = pct(stats.taproot_spend_count); let taproot_output_pct = pct(stats.taproot_output_count);
    let proto_color = |count: usize| -> Color { if count > 0 { Color::Yellow } else { Color::DarkGray } };
    let viol_color = |count: usize| -> Color { if count > 0 { Color::Red } else { Color::Green } };

    let protocols: Vec<(&str, usize, u64, usize, &str)> = vec![
        ("Runes", stats.rune_count, stats.rune_vsize, stats.rune_bip110v, "fungible tokens via OP_RETURN"),
        ("BRC-20", stats.brc20_count, stats.brc20_vsize, stats.brc20_bip110v, "token standard via ordinals"),
        ("Inscriptions", stats.inscription_count, stats.inscription_vsize, stats.inscription_bip110v, "ordinals data (images, text, etc.)"),
        ("OPNET", stats.opnet_count, stats.opnet_vsize, stats.opnet_bip110v, "smart contracts via tapscript"),
        ("Stamps", stats.stamp_count, stats.stamp_vsize, stats.stamp_bip110v, "SRC-20 tokens via bare multisig"),
        ("Counterparty", stats.counterparty_count, stats.counterparty_vsize, stats.counterparty_bip110v, "asset protocol (XCP)"),
        ("Omni Layer", stats.omni_count, stats.omni_vsize, stats.omni_bip110v, "token layer (ex-Mastercoin)"),
        ("OP_RETURN other", stats.opreturn_other_count, stats.opreturn_other_vsize, stats.opreturn_other_bip110v, "unclassified nulldata"),
        ("Other", stats.other_data_count, stats.other_data_vsize, 0, "data tx, unknown protocol"),
    ];

    let mut text = vec![
        Line::from(vec![Span::styled("Total output:    ", Style::default().fg(Color::DarkGray)), Span::styled(format_btc(stats.total_out), Style::default().fg(Color::White).bold())]),
        Line::from(vec![Span::styled("Total fees:      ", Style::default().fg(Color::DarkGray)), Span::styled(format_btc_fees(stats.total_fee), Style::default().fg(Color::White).bold())]),
        Line::from(vec![Span::styled("Transactions:    ", Style::default().fg(Color::DarkGray)), Span::styled(format!("{}", user_tx), Style::default().fg(Color::White).bold()), Span::styled("  (excl. coinbase)", Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled("Total weight:    ", Style::default().fg(Color::DarkGray)), Span::styled(format!("{} WU ({:.1}%)", format_number(block.weight), block.weight as f64 / 4_000_000.0 * 100.0), Style::default().fg(Color::White).bold())]),
        Line::from(""),
        Line::from(Span::styled("Transaction Breakdown", Style::default().fg(Color::Cyan).bold())),
        Line::from(vec![
            Span::styled("  Financial:       ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{} ({:.1}%)", stats.financial_count, fin_pct), Style::default().fg(Color::Green).bold()),
            Span::styled(format!("  {:>5}% wt", format!("{:.1}", if stats.total_vsize > 0 { stats.financial_vsize as f64 / stats.total_vsize as f64 * 100.0 } else { 0.0 })), Style::default().fg(Color::Green)),
            Span::styled(if stats.financial_count > 0 { format!("  110:{:>6}", format!("{}/{}", stats.financial_count.saturating_sub(stats.financial_bip110v), stats.financial_count)) } else { "  110:     -".to_string() }, Style::default().fg(if stats.financial_bip110v == 0 { Color::Green } else { Color::Yellow })),
        ]),
        Line::from(vec![
            Span::styled("  Data/spam:       ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{} ({:.1}%)", data_count, data_pct), Style::default().fg(if data_pct > 10.0 { Color::Red } else { Color::Yellow }).bold()),
            Span::styled(format!("  {:>5}% wt", format!("{:.1}", if stats.total_vsize > 0 { data_vsize as f64 / stats.total_vsize as f64 * 100.0 } else { 0.0 })), Style::default().fg(if data_pct > 10.0 { Color::Red } else { Color::Yellow })),
        ]),
        Line::from(""), Line::from(Span::styled("Protocol Breakdown", Style::default().fg(Color::Cyan).bold())),
    ];
    let vsize_pct = |vs: u64| -> f64 { if stats.total_vsize > 0 { (vs as f64 / stats.total_vsize as f64) * 100.0 } else { 0.0 } };
    for (label, count, vsize, violations, desc) in &protocols {
        let compliant = count.saturating_sub(*violations);
        let bip110_str = if *count > 0 { format!("{}/{}", compliant, count) } else { "-".to_string() };
        let bip110_color = if *count == 0 { Color::DarkGray } else if *violations == 0 { Color::Green } else if compliant == 0 { Color::Red } else { Color::Yellow };
        text.push(Line::from(vec![
            Span::styled(format!("  {:17}", format!("{}:", label)), Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:>4} ({:.1}%)", count, pct(*count)), Style::default().fg(proto_color(*count))),
            Span::styled(format!("  {:>5}% wt", format!("{:.1}", vsize_pct(*vsize))), Style::default().fg(proto_color(*count))),
            Span::styled(format!("  110:{:>6}", bip110_str), Style::default().fg(bip110_color)),
            Span::styled(format!("  {}", desc), Style::default().fg(Color::DarkGray)),
        ]));
    }
    text.extend_from_slice(&[
        Line::from(""), Line::from(Span::styled("Taproot Usage", Style::default().fg(Color::Cyan).bold())),
        Line::from(vec![Span::styled("  Spending from:   ", Style::default().fg(Color::DarkGray)), Span::styled(format!("{} ({:.1}%)", stats.taproot_spend_count, taproot_spend_pct), Style::default().fg(proto_color(stats.taproot_spend_count)))]),
        Line::from(vec![Span::styled("  Creating to:     ", Style::default().fg(Color::DarkGray)), Span::styled(format!("{} ({:.1}%)", stats.taproot_output_count, taproot_output_pct), Style::default().fg(proto_color(stats.taproot_output_count)))]),
        Line::from(""), Line::from(Span::styled("BIP-110 Compliance", Style::default().fg(Color::Cyan).bold())),
        Line::from(vec![Span::styled("  Compliant txs:   ", Style::default().fg(Color::DarkGray)), Span::styled(format!("{} ({:.1}%)", user_tx.saturating_sub(stats.bip110_violating_txs), if user_tx > 0 { (user_tx.saturating_sub(stats.bip110_violating_txs)) as f64 / user_tx as f64 * 100.0 } else { 100.0 }), Style::default().fg(if stats.bip110_violating_txs == 0 { Color::Green } else { Color::Yellow }).bold())]),
        Line::from(Span::styled("  Violations:", Style::default().fg(Color::DarkGray))),
        Line::from(vec![Span::styled("    OP_RETURN >83B:  ", Style::default().fg(Color::DarkGray)), Span::styled(format!("{}", stats.oversized_opreturn_count), Style::default().fg(viol_color(stats.oversized_opreturn_count))), Span::styled("  Largest: ", Style::default().fg(Color::DarkGray)), Span::styled(if stats.max_opreturn_size > 0 { format!("{} bytes", stats.max_opreturn_size) } else { "n/a".to_string() }, Style::default().fg(if stats.max_opreturn_size > 83 { Color::Red } else { Color::White }))]),
        Line::from(vec![Span::styled("    scriptPubKey>34B:", Style::default().fg(Color::DarkGray)), Span::styled(format!(" {}", stats.bip110_oversized_spk), Style::default().fg(viol_color(stats.bip110_oversized_spk)))]),
        Line::from(vec![Span::styled("    Witness >256B:   ", Style::default().fg(Color::DarkGray)), Span::styled(format!("{}", stats.bip110_oversized_pushdata), Style::default().fg(viol_color(stats.bip110_oversized_pushdata)))]),
        Line::from(vec![Span::styled("    OP_SUCCESS:      ", Style::default().fg(Color::DarkGray)), Span::styled(format!("{}", stats.bip110_op_success), Style::default().fg(viol_color(stats.bip110_op_success)))]),
        Line::from(vec![Span::styled("    OP_IF in tapscript:", Style::default().fg(Color::DarkGray)), Span::styled(format!(" {}", stats.bip110_op_if), Style::default().fg(viol_color(stats.bip110_op_if)))]),
        Line::from(""), Line::from(Span::styled("↑/↓: prev/next block | Esc: close", Style::default().fg(Color::DarkGray))),
    ]);

    let title = format!(" Block {} ", format_number(block.height));
    let modal_block = Block::default().borders(Borders::ALL).border_type(BorderType::Double)
        .title(title).title_style(Style::default().fg(Color::Cyan).bold()).style(Style::default().bg(Color::Black));
    f.render_widget(Clear, modal_area);
    f.render_widget(Paragraph::new(text).block(modal_block).wrap(Wrap { trim: false }), modal_area);
}

fn draw_blockchain_card(f: &mut Frame, area: Rect, data: &NodeData) {
    let bc = &data.blockchain; let progress = bc.verificationprogress * 100.0;
    let synced = if progress >= 99.99 { "YES" } else { "syncing" };
    let lines = vec![
        Line::from(vec![Span::styled("Height:  ", Style::default().fg(Color::DarkGray)), Span::styled(format_number(bc.blocks), Style::default().fg(Color::Yellow).bold())]),
        Line::from(vec![Span::styled("Headers: ", Style::default().fg(Color::DarkGray)), Span::styled(format_number(bc.headers), Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled("Synced:  ", Style::default().fg(Color::DarkGray)), Span::styled(format!("{} ({:.2}%)", synced, progress), Style::default().fg(if progress >= 99.99 { Color::Green } else { Color::Yellow }))]),
        Line::from(vec![Span::styled("Diff:    ", Style::default().fg(Color::DarkGray)), Span::styled(format!("{:.2e}", bc.difficulty), Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled("Disk:    ", Style::default().fg(Color::DarkGray)), Span::styled(format_bytes(bc.size_on_disk), Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled("Pruned:  ", Style::default().fg(Color::DarkGray)), Span::styled(if bc.pruned { "yes" } else { "no" }.to_string(), Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled("IBD:     ", Style::default().fg(Color::DarkGray)), Span::styled(if bc.initialblockdownload { "yes" } else { "no" }.to_string(), Style::default().fg(if bc.initialblockdownload { Color::Yellow } else { Color::Green }))]),
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
    let mut all_lines = vec![
        Line::from(vec![Span::styled("Conns:    ", Style::default().fg(Color::DarkGray)), Span::styled(format!("{} (in: {} / out: {})", net.connections, net.connections_in, net.connections_out), Style::default().fg(Color::Yellow).bold())]),
        Line::from(vec![Span::styled("Protocol: ", Style::default().fg(Color::DarkGray)), Span::styled(format!("{}", net.protocolversion), Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled("Recv:     ", Style::default().fg(Color::DarkGray)), Span::styled(format_bytes(nt.totalbytesrecv), Style::default().fg(Color::Green))]),
        Line::from(vec![Span::styled("Sent:     ", Style::default().fg(Color::DarkGray)), Span::styled(format_bytes(nt.totalbytessent), Style::default().fg(Color::Red))]),
        Line::from(vec![Span::styled("Relay fee:", Style::default().fg(Color::DarkGray)), Span::styled(format!(" {:.8} BTC/kvB", net.relayfee), Style::default().fg(Color::White))]),
        Line::from(vec![Span::styled("Incr fee: ", Style::default().fg(Color::DarkGray)), Span::styled(format!("{:.8} BTC/kvB", net.incrementalfee), Style::default().fg(Color::White))]),
    ];
    if !net.localaddresses.is_empty() {
        let addrs: Vec<String> = net.localaddresses.iter().map(|a| format!("{}:{}", a.address, a.port)).collect();
        all_lines.push(Line::from(vec![Span::styled("Local:    ", Style::default().fg(Color::DarkGray)), Span::styled(addrs.join(", "), Style::default().fg(Color::White))]));
    }
    f.render_widget(Paragraph::new(all_lines).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" Network ").title_style(Style::default().fg(Color::Green).bold())), area);
}

fn draw_mining_card(f: &mut Frame, area: Rect, data: &NodeData) {
    let mi = &data.mining; let warnings_text = data.blockchain.warnings.as_str();
    let mut lines = vec![
        Line::from(vec![Span::styled("Hashrate: ", Style::default().fg(Color::DarkGray)), Span::styled(format_hashrate(mi.networkhashps), Style::default().fg(Color::Yellow).bold())]),
        Line::from(vec![Span::styled("Pooled TX:", Style::default().fg(Color::DarkGray)), Span::styled(format!(" {}", mi.pooledtx), Style::default().fg(Color::White))]),
    ];
    if !warnings_text.is_empty() {
        lines.push(Line::from("")); lines.push(Line::from(Span::styled("Warnings:", Style::default().fg(Color::Red).bold())));
        for chunk in warnings_text.as_bytes().chunks(area.width.saturating_sub(4) as usize) {
            if let Ok(s) = std::str::from_utf8(chunk) { lines.push(Line::from(Span::styled(s.to_string(), Style::default().fg(Color::Red)))); }
        }
    }
    f.render_widget(Paragraph::new(lines).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" Mining / Warnings ").title_style(Style::default().fg(Color::Yellow).bold())), area);
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
            Cell::from(Span::styled(marker, Style::default().fg(Color::Yellow))), Cell::from(format_number(b.height)), Cell::from(timestamp), Cell::from(format_number(b.tx_count as u64)), Cell::from(format_bytes_short(b.size)), Cell::from(format!("{:.1} kvWU", b.weight as f64 / 1000.0)), Cell::from(age), Cell::from(Span::styled(bip110.to_string(), Style::default().fg(bip110_color))), Cell::from(btc_out), Cell::from(fees), Cell::from(Span::styled(financial, Style::default().fg(fin_color))), Cell::from(Line::from(Span::styled(viol_count, Style::default().fg(viol_color))).alignment(Alignment::Right)), Cell::from(Span::styled(viol_pct_str, Style::default().fg(viol_color))),
        ])
    }).collect();
    let widths = vec![Constraint::Length(2), Constraint::Length(10), Constraint::Length(12), Constraint::Length(7), Constraint::Length(10), Constraint::Length(12), Constraint::Length(12), Constraint::Length(7), Constraint::Length(12), Constraint::Length(12), Constraint::Length(5), Constraint::Length(5), Constraint::Min(5)];
    let total = data.recent_blocks.len();
    let title = format!(" Recent Blocks ({}-{}/{}) [Enter: detail] ", scroll + 1, (scroll + 8).min(total), total);
    let border_color = if focused { Color::Yellow } else { Color::default() };
    f.render_widget(Table::new(rows, widths).header(header).block(
        Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(border_color)).title(title).title_style(Style::default().fg(Color::Yellow).bold())), area);
}
