use crate::rpc::NodeData;
use crate::service::AppService;
use crossterm::event::KeyCode;
use ratatui::{prelude::*, widgets::*};
use std::collections::BTreeMap;

use super::common::format_number;
use super::{KeyResult, Screen, SharedState};

pub struct KnownPeersScreen {
    scroll: u16,
}

impl KnownPeersScreen {
    pub fn new() -> Self {
        Self { scroll: 0 }
    }
}

impl Screen for KnownPeersScreen {
    fn name(&self) -> &str { "Known Peers" }

    fn footer_hint(&self) -> &str {
        " q: quit | Tab: signaling | ↑/↓: scroll services | r: refresh "
    }

    fn draw(&self, f: &mut Frame, area: Rect, state: &SharedState) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(10),
                Constraint::Min(8),
            ])
            .split(area);

        draw_known_peers_time(f, layout[0], &state.node_data);
        draw_known_peers_services(f, layout[1], &state.node_data, self.scroll);
    }

    fn handle_key(&mut self, key: KeyCode, _state: &mut SharedState, svc: &AppService) -> KeyResult {
        match key {
            KeyCode::Down => { self.scroll = self.scroll.saturating_add(1); KeyResult::None }
            KeyCode::Up => { self.scroll = self.scroll.saturating_sub(1); KeyResult::None }
            KeyCode::Char('r') => { svc.force_refresh(); KeyResult::None }
            KeyCode::Esc => KeyResult::Quit,
            _ => KeyResult::None,
        }
    }

    fn on_enter(&self, svc: &AppService) {
        svc.notify_poll();
    }
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
        Constraint::Length(7), Constraint::Length(7), Constraint::Length(7),
        Constraint::Length(7), Constraint::Length(7), Constraint::Length(7),
        Constraint::Length(7), Constraint::Length(7), Constraint::Length(7),
        Constraint::Length(7), Constraint::Length(10),
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

fn draw_known_peers_services(f: &mut Frame, area: Rect, data: &NodeData, scroll: u16) {
    let local_services: u64 = u64::from_str_radix(
        data.network.localservices.trim_start_matches("0x"),
        16,
    )
    .unwrap_or(0);

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

    let mut header_cells = vec!["Service".to_string()];
    for net in &networks {
        header_cells.push(net.clone());
    }
    header_cells.push("TOTAL".to_string());
    header_cells.push("Description".to_string());

    let header = Row::new(header_cells)
        .style(Style::default().fg(Color::Cyan).bold())
        .bottom_margin(0);

    let grand_total: u64 = by_network.values().map(|(t, _)| t).sum();

    let mut bit_totals: Vec<(usize, u64)> = active_bits
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let total: u64 = networks.iter().map(|net| by_network[net].1[i]).sum();
            (i, total)
        })
        .collect();
    bit_totals.sort_by(|a, b| b.1.cmp(&a.1));

    let rows: Vec<Row> = bit_totals
        .iter()
        .map(|&(i, bit_total)| {
            let bit = active_bits[i];
            let is_local = local_services & (1u64 << bit) != 0;
            let known_name = service_bit_name(bit);
            let name = if is_local {
                if known_name.is_empty() { format!("* bit{}", bit) } else { format!("* {}", known_name) }
            } else if known_name.is_empty() {
                format!("  bit{}", bit)
            } else {
                format!("  {}", known_name)
            };
            let mut cells = vec![name];
            for net in &networks {
                let (net_total, bit_counts) = &by_network[net];
                cells.push(format_count_pct(bit_counts[i], *net_total));
            }
            cells.push(format_count_pct(bit_total, grand_total));
            cells.push(service_bit_desc(bit).to_string());
            let color = if is_local { Color::Green } else { Color::White };
            Row::new(cells).style(Style::default().fg(color))
        })
        .collect();

    let mut widths = vec![Constraint::Length(24)];
    for _ in &networks { widths.push(Constraint::Length(14)); }
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

    let mut tstate = TableState::default().with_offset(scroll as usize);
    f.render_stateful_widget(table, area, &mut tstate);
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
