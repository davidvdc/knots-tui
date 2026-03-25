use crate::rpc::NodeData;
use crate::service::AppService;
use crossterm::event::KeyCode;
use ratatui::{prelude::*, widgets::*};
use std::collections::BTreeMap;
use std::sync::Arc;

use super::common::format_number;
use super::{KeyResult, Screen, StateRef};

pub struct KnownPeersScreen {
    svc: Arc<AppService>,
    state: StateRef,
    scroll: u16,
}

impl KnownPeersScreen {
    pub fn new(svc: Arc<AppService>, state: StateRef) -> Self {
        Self { svc, state, scroll: 0 }
    }
}

impl Screen for KnownPeersScreen {
    fn name(&self) -> &str { "Known Peers" }
    fn footer_hint(&self) -> &str { " q: quit | Tab: signaling | ↑/↓: scroll services | r: refresh " }

    fn draw(&self, f: &mut Frame, area: Rect) {
        let state = self.state.borrow();
        let layout = Layout::default().direction(Direction::Vertical)
            .constraints([Constraint::Length(10), Constraint::Min(8)]).split(area);
        draw_known_peers_time(f, layout[0], &state.node_data);
        draw_known_peers_services(f, layout[1], &state.node_data, self.scroll);
    }

    fn handle_key(&mut self, key: KeyCode) -> KeyResult {
        match key {
            KeyCode::Down => { self.scroll = self.scroll.saturating_add(1); KeyResult::None }
            KeyCode::Up => { self.scroll = self.scroll.saturating_sub(1); KeyResult::None }
            KeyCode::Char('r') => { self.svc.set_loading(true); self.svc.fetch_known_peers(); KeyResult::None }
            KeyCode::Esc => KeyResult::Quit,
            _ => KeyResult::None,
        }
    }

    fn on_enter(&mut self) {
        self.svc.set_loading(true);
        self.svc.stop_polling();
        self.svc.fetch_known_peers();
    }
}

fn net_color(net: &str) -> Color {
    match net { "ipv4" => Color::Green, "ipv6" => Color::Blue, "onion" => Color::Magenta, "i2p" => Color::Yellow, "cjdns" => Color::Cyan, _ => Color::White }
}

fn draw_known_peers_time(f: &mut Frame, area: Rect, data: &NodeData) {
    let header = Row::new(vec!["Network", "<1h", "<4h", "<24h", "<7d", "<30d", "<90d", ">90d", "Total"])
        .style(Style::default().fg(Color::Cyan).bold()).bottom_margin(0);
    let now = data.fetched_at;
    let cutoffs: &[u64] = &[3600, 14400, 86400, 604800, 2592000, 7776000];
    let mut by_net: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for addr in &data.known_addresses {
        let net = if addr.network.is_empty() { "unknown".to_string() } else { addr.network.clone() };
        let buckets = by_net.entry(net).or_insert_with(|| vec![0u64; cutoffs.len() + 1]);
        let age = now.saturating_sub(addr.time);
        let mut placed = false;
        for (i, &c) in cutoffs.iter().enumerate() { if age < c { buckets[i] += 1; placed = true; break; } }
        if !placed { buckets[cutoffs.len()] += 1; }
    }
    let rows: Vec<Row> = by_net.iter().map(|(net, buckets)| {
        let total: u64 = buckets.iter().sum();
        let mut cells: Vec<String> = vec![net.clone()];
        for b in buckets { cells.push(format_number(*b)); }
        cells.push(format_number(total));
        Row::new(cells).style(Style::default().fg(net_color(net)))
    }).collect();
    let widths = [Constraint::Length(10), Constraint::Length(7), Constraint::Length(7), Constraint::Length(7), Constraint::Length(7), Constraint::Length(7), Constraint::Length(7), Constraint::Length(7), Constraint::Length(10)];
    f.render_widget(Table::new(rows, widths).header(header).block(
        Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)
            .title(format!(" Known Addresses ({}) by Last Seen ", format_number(data.known_peers)))
            .title_style(Style::default().fg(Color::Cyan).bold())), area);
}

fn format_count_pct(count: u64, total: u64) -> String {
    if total == 0 { return "0".to_string(); }
    format!("{} ({:.0}%)", format_number(count), (count as f64 / total as f64) * 100.0)
}

fn service_bit_name(bit: u8) -> &'static str {
    match bit {
        0 => "NODE_NETWORK", 1 => "NODE_GETUTXO", 2 => "NODE_BLOOM",
        3 => "NODE_WITNESS", 4 => "NODE_XTHIN", 5 => "NODE_BITCOIN_CASH",
        6 => "NODE_COMPACT_FILTERS", 10 => "NODE_NETWORK_LIMITED",
        24 => "NODE_P2P_V2", 27 => "NODE_REDUCED_DATA",
        _ if bit >= 24 => "experimental",
        _ => "",
    }
}

fn service_bit_desc(bit: u8) -> &'static str {
    match bit {
        0 => "Serves all blocks since genesis",
        1 => "Serves UTXO queries (BIP64)",
        2 => "SPV bloom filter queries (BIP111)",
        3 => "Understands SegWit (BIP144)",
        4 => "Xtreme Thinblocks (Bitcoin Unlimited)",
        5 => "Bitcoin Cash fork identifier (stale)",
        6 => "Serves BIP157 compact block filters",
        10 => "Pruned node, serves last 288 blocks only",
        24 => "Encrypted P2P via v2 transport (BIP324)",
        27 => "Enforces BIP-110 ReducedData rules",
        _ if bit >= 24 => "Reserved for temporary experiments (bits 24-31)",
        _ => "",
    }
}

fn draw_known_peers_services(f: &mut Frame, area: Rect, data: &NodeData, scroll: u16) {
    let local_services: u64 = u64::from_str_radix(data.network.localservices.trim_start_matches("0x"), 16).unwrap_or(0);
    let mut all_bits_mask: u64 = 0;
    for addr in &data.known_addresses { all_bits_mask |= addr.services; }
    let mut active_bits: Vec<u8> = Vec::new();
    for bit in 0..64u8 { if all_bits_mask & (1u64 << bit) != 0 { active_bits.push(bit); } }
    let mut by_network: BTreeMap<String, (u64, Vec<u64>)> = BTreeMap::new();
    for addr in &data.known_addresses {
        let net = if addr.network.is_empty() { "unknown".to_string() } else { addr.network.clone() };
        let entry = by_network.entry(net).or_insert_with(|| (0, vec![0u64; active_bits.len()]));
        entry.0 += 1;
        for (i, &bit) in active_bits.iter().enumerate() { if addr.services & (1u64 << bit) != 0 { entry.1[i] += 1; } }
    }
    let networks: Vec<String> = by_network.keys().cloned().collect();
    let mut header_cells = vec!["Service".to_string()];
    for net in &networks { header_cells.push(net.clone()); }
    header_cells.push("TOTAL".to_string()); header_cells.push("Description".to_string());
    let header = Row::new(header_cells).style(Style::default().fg(Color::Cyan).bold()).bottom_margin(0);
    let grand_total: u64 = by_network.values().map(|(t, _)| t).sum();
    let mut bit_totals: Vec<(usize, u64)> = active_bits.iter().enumerate()
        .map(|(i, _)| { let total: u64 = networks.iter().map(|net| by_network[net].1[i]).sum(); (i, total) }).collect();
    bit_totals.sort_by(|a, b| b.1.cmp(&a.1));
    let rows: Vec<Row> = bit_totals.iter().map(|&(i, bit_total)| {
        let bit = active_bits[i]; let is_local = local_services & (1u64 << bit) != 0;
        let known_name = service_bit_name(bit);
        let name = if is_local { if known_name.is_empty() { format!("* bit{}", bit) } else { format!("* {}", known_name) } }
            else if known_name.is_empty() { format!("  bit{}", bit) } else { format!("  {}", known_name) };
        let mut cells = vec![name];
        for net in &networks { let (net_total, bit_counts) = &by_network[net]; cells.push(format_count_pct(bit_counts[i], *net_total)); }
        cells.push(format_count_pct(bit_total, grand_total)); cells.push(service_bit_desc(bit).to_string());
        Row::new(cells).style(Style::default().fg(if is_local { Color::Green } else { Color::White }))
    }).collect();
    let mut widths = vec![Constraint::Length(24)];
    for _ in &networks { widths.push(Constraint::Length(14)); }
    widths.push(Constraint::Length(14)); widths.push(Constraint::Min(24));
    let table = Table::new(rows, widths).header(header).block(
        Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)
            .title(" Services by Network (* = this node) [↑/↓ scroll] ")
            .title_style(Style::default().fg(Color::Yellow).bold()));
    let mut tstate = TableState::default().with_offset(scroll as usize);
    f.render_stateful_widget(table, area, &mut tstate);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn service_bit_known() { assert_eq!(service_bit_name(0), "NODE_NETWORK"); assert_eq!(service_bit_name(3), "NODE_WITNESS"); assert_eq!(service_bit_name(10), "NODE_NETWORK_LIMITED"); assert_eq!(service_bit_name(24), "NODE_P2P_V2"); }
    #[test] fn service_bit_unknown() { assert_eq!(service_bit_name(5), ""); assert_eq!(service_bit_name(63), ""); }
}
