use crate::rpc::NodeData;
use crate::Screen;
use ratatui::{prelude::*, widgets::*};
use std::collections::BTreeMap;

pub fn draw(f: &mut Frame, data: &NodeData, peer_scroll: u16, block_scroll: u16, screen: Screen) {
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
        Screen::Dashboard => draw_body(f, outer[1], data, peer_scroll, block_scroll),
        Screen::KnownPeers => draw_known_peers(f, outer[1], data),
    }
    draw_footer(f, outer[2], screen);
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

fn draw_body(f: &mut Frame, area: Rect, data: &NodeData, peer_scroll: u16, block_scroll: u16) {
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

    // Bottom: recent blocks then peers, stacked vertically
    let bottom_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(11), // 8 blocks + header + borders
            Constraint::Min(8),    // peers fills the rest
        ])
        .split(rows[1]);

    draw_blocks_table(f, bottom_rows[0], data, block_scroll);
    draw_peers_table(f, bottom_rows[1], data, peer_scroll);
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

fn draw_peers_table(f: &mut Frame, area: Rect, data: &NodeData, scroll: u16) {
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

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(format!(
                    " Peers ({}) | known: {} [j/k scroll] ",
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

fn draw_blocks_table(f: &mut Frame, area: Rect, data: &NodeData, block_scroll: u16) {
    let header = Row::new(vec!["Height", "TXs", "Size", "Weight", "Age"])
        .style(Style::default().fg(Color::Cyan).bold())
        .bottom_margin(0);

    let now = chrono::Utc::now().timestamp() as u64;

    let rows: Vec<Row> = data
        .recent_blocks
        .iter()
        .map(|b| {
            let age = if b.time > 0 && now > b.time {
                format_duration(now - b.time)
            } else {
                "-".to_string()
            };
            Row::new(vec![
                format_number(b.height),
                format_number(b.tx_count as u64),
                format_bytes_short(b.size),
                format!("{:.1} kvWU", b.weight as f64 / 1000.0),
                age,
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(10),
        Constraint::Length(7),
        Constraint::Length(10),
        Constraint::Length(12),
        Constraint::Min(8),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Recent Blocks [J/K scroll] ")
                .title_style(Style::default().fg(Color::Yellow).bold()),
        );

    let mut state = TableState::default().with_offset(block_scroll as usize);
    f.render_stateful_widget(table, area, &mut state);
}

fn draw_known_peers(f: &mut Frame, area: Rect, data: &NodeData) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10), // time buckets table
            Constraint::Min(8),    // services table
        ])
        .split(area);

    draw_known_peers_time(f, layout[0], data);
    draw_known_peers_services(f, layout[1], data);
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

fn service_bit_name(bit: u8) -> String {
    match bit {
        0 => "Full".to_string(),
        1 => "UTXO".to_string(),
        2 => "Bloom".to_string(),
        3 => "SegWit".to_string(),
        6 => "CmpctFlt".to_string(),
        10 => "Pruned".to_string(),
        24 => "v2Trans".to_string(),
        n => format!("bit{}", n),
    }
}

fn draw_known_peers_services(f: &mut Frame, area: Rect, data: &NodeData) {
    // Discover all service bits present in the dataset
    let mut all_bits_mask: u64 = 0;
    for addr in &data.known_addresses {
        all_bits_mask |= addr.services;
    }

    // Collect active bit positions, sorted
    let mut active_bits: Vec<u8> = Vec::new();
    for bit in 0..64u8 {
        if all_bits_mask & (1u64 << bit) != 0 {
            active_bits.push(bit);
        }
    }

    // Count per network per bit
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

    // Header
    let mut header_cells = vec!["Network".to_string(), "Total".to_string()];
    for &bit in &active_bits {
        header_cells.push(service_bit_name(bit));
    }

    let header = Row::new(header_cells)
        .style(Style::default().fg(Color::Cyan).bold())
        .bottom_margin(0);

    // Aggregate totals
    let mut agg_total: u64 = 0;
    let mut agg_bits = vec![0u64; active_bits.len()];

    let mut rows: Vec<Row> = by_network
        .iter()
        .map(|(net, (total, bit_counts))| {
            agg_total += total;
            for (i, &count) in bit_counts.iter().enumerate() {
                agg_bits[i] += count;
            }

            let mut cells = vec![net.clone(), format_number(*total)];
            for (i, &count) in bit_counts.iter().enumerate() {
                let _ = i;
                cells.push(format_count_pct(count, *total));
            }
            Row::new(cells).style(Style::default().fg(net_color(net)))
        })
        .collect();

    // Totals row
    let mut total_cells = vec!["TOTAL".to_string(), format_number(agg_total)];
    for &count in &agg_bits {
        total_cells.push(format_count_pct(count, agg_total));
    }
    rows.push(
        Row::new(total_cells)
            .style(Style::default().fg(Color::White).bold()),
    );

    // Widths: network + total + one per active bit
    let mut widths = vec![Constraint::Length(10), Constraint::Length(10)];
    for _ in &active_bits {
        widths.push(Constraint::Length(14));
    }

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Services by Network ")
                .title_style(Style::default().fg(Color::Yellow).bold()),
        );

    f.render_widget(table, area);
}

fn draw_footer(f: &mut Frame, area: Rect, screen: Screen) {
    let text = match screen {
        Screen::Dashboard => " q: quit | Tab: known peers | j/k: scroll peers | J/K: scroll blocks ",
        Screen::KnownPeers => " q: quit | Tab: dashboard ",
    };
    let footer = Paragraph::new(text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(footer, area);
}

// --- Formatting helpers ---

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
