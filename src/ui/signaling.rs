use crate::rpc::NodeData;
use crate::service::AppService;
use crossterm::event::KeyCode;
use ratatui::{prelude::*, widgets::*};
use std::collections::BTreeMap;
use std::sync::Arc;

use super::common::format_number;
use super::{KeyResult, Screen, StateRef};

pub struct SignalingScreen {
    svc: Arc<AppService>,
    state: StateRef,
    selected_bit: u8,
    show_bit_modal: bool,
}

impl SignalingScreen {
    pub fn new(svc: Arc<AppService>, state: StateRef) -> Self {
        Self { svc, state, selected_bit: 0, show_bit_modal: false }
    }
}

impl Screen for SignalingScreen {
    fn name(&self) -> &str { "Signaling" }

    fn footer_hint(&self) -> &str {
        " q: quit | Tab: analytics | ↑/↓: select bit | Enter: details | r: refresh "
    }

    fn draw(&self, f: &mut Frame, area: Rect) {
        let state = self.state.borrow();
        let data = &state.signaling_data;
        // Merge softforks from both dashboard (fast) and signaling (slow) sources
        // so node-reported forks like reduced_data appear as soon as dashboard loads
        let mut softforks = state.node_data.blockchain.softforks.clone();
        for (name, fork) in &data.softforks {
            softforks.insert(name.clone(), fork.clone());
        }
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Length(14)])
            .split(area);

        draw_version_bits(f, layout[0], data, self.selected_bit);
        draw_softforks(f, layout[1], &softforks);
    }

    fn handle_key(&mut self, key: KeyCode) -> KeyResult {
        match key {
            KeyCode::Down => { self.selected_bit = (self.selected_bit + 1).min(28); KeyResult::None }
            KeyCode::Up => { self.selected_bit = self.selected_bit.saturating_sub(1); KeyResult::None }
            KeyCode::Enter => { self.show_bit_modal = true; KeyResult::None }
            KeyCode::Char('r') => { self.svc.set_loading(true); self.svc.notify_signaling(); KeyResult::None }
            KeyCode::Esc => KeyResult::Quit,
            _ => KeyResult::None,
        }
    }

    fn has_modal(&self) -> bool { self.show_bit_modal }

    fn draw_modal(&self, f: &mut Frame, area: Rect) {
        let state = self.state.borrow();
        draw_bit_modal(f, area, self.selected_bit, &state.signaling_data);
    }

    fn handle_modal_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => { self.show_bit_modal = false; }
            _ => {}
        }
    }

    fn on_enter(&mut self) {
        self.svc.set_loading(true);
        self.svc.stop_polling();
        self.svc.notify_signaling();
    }
}

fn draw_bit_modal(f: &mut Frame, area: Rect, selected_bit: u8, data: &NodeData) {
    let modal_width = (area.width as f32 * 0.65) as u16;
    let modal_height = (area.height as f32 * 0.6) as u16;
    let x = (area.width.saturating_sub(modal_width)) / 2;
    let y = (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(x, y, modal_width, modal_height);

    let dim = Block::default().style(Style::default().bg(Color::Black));
    f.render_widget(Clear, modal_area);
    f.render_widget(dim, area);

    let (title, detail) = bit_detail(selected_bit);

    let total_blocks = data.recent_block_versions.len() as u64;
    let count = data.recent_block_versions.iter()
        .filter(|&&(_, v)| v >= 0x20000000 && v & (1i64 << selected_bit) != 0)
        .count() as u64;
    let pct = if total_blocks > 0 { (count as f64 / total_blocks as f64) * 100.0 } else { 0.0 };

    let stats_line = format!("Signaling: {}/{} blocks ({:.1}%)\n", format_number(count), format_number(total_blocks), pct);

    let mut text = vec![
        Line::from(Span::styled(stats_line, Style::default().fg(Color::Yellow).bold())),
        Line::from(""),
    ];
    for line in detail.lines() {
        text.push(Line::from(Span::styled(line.to_string(), Style::default().fg(Color::White))));
    }
    text.push(Line::from(""));
    text.push(Line::from(Span::styled("Press Esc or Enter to close", Style::default().fg(Color::DarkGray))));

    let block = Block::default()
        .borders(Borders::ALL).border_type(BorderType::Double)
        .title(format!(" {} ", title))
        .title_style(Style::default().fg(Color::Cyan).bold())
        .style(Style::default().bg(Color::Black));

    let paragraph = Paragraph::new(text).block(block).wrap(Wrap { trim: false });
    f.render_widget(Clear, modal_area);
    f.render_widget(paragraph, modal_area);
}

fn known_buried_softforks() -> BTreeMap<String, crate::rpc::SoftFork> {
    use crate::rpc::SoftFork;
    let mut m = BTreeMap::new();
    let buried = |height: i64| SoftFork { fork_type: "buried".to_string(), active: true, height: Some(height), bip9: None };
    m.insert("bip34".to_string(), buried(227931));
    m.insert("bip66".to_string(), buried(363725));
    m.insert("bip65".to_string(), buried(388381));
    m.insert("csv".to_string(), buried(419328));
    m.insert("segwit".to_string(), buried(481824));
    m.insert("taproot".to_string(), buried(709632));
    m
}

fn draw_softforks(f: &mut Frame, area: Rect, softforks: &std::collections::BTreeMap<String, crate::rpc::SoftFork>) {
    let header = Row::new(vec!["Name", "Type", "Active", "Height", "Status", "Bit", "Progress"])
        .style(Style::default().fg(Color::Cyan).bold()).bottom_margin(0);

    let mut merged = known_buried_softforks();
    for (name, fork) in softforks { merged.insert(name.clone(), fork.clone()); }

    let mut sorted: Vec<(&String, &crate::rpc::SoftFork)> = merged.iter().collect();
    sorted.sort_by(|(_, a), (_, b)| {
        let a_buried = a.fork_type == "buried";
        let b_buried = b.fork_type == "buried";
        match (a_buried, b_buried) {
            (false, true) => std::cmp::Ordering::Less,
            (true, false) => std::cmp::Ordering::Greater,
            _ => { let ah = a.height.unwrap_or(i64::MAX); let bh = b.height.unwrap_or(i64::MAX); bh.cmp(&ah) }
        }
    });

    let rows: Vec<Row> = sorted.iter().map(|(name, fork)| {
        let is_buried = fork.fork_type == "buried";
        let active_str = if fork.active { "yes" } else { "no" };
        let height_str = fork.height.map(|h| format_number(h as u64)).unwrap_or_else(|| "-".to_string());
        let (status, bit, progress) = if let Some(ref bip9) = fork.bip9 {
            let bit_str = bip9.bit.map(|b| b.to_string()).unwrap_or_else(|| "-".to_string());
            let progress = if let Some(ref stats) = bip9.statistics {
                format!("{}/{} ({:.1}%)", format_number(stats.count), format_number(stats.period),
                    if stats.period > 0 { (stats.count as f64 / stats.period as f64) * 100.0 } else { 0.0 })
            } else { "-".to_string() };
            (bip9.status.clone(), bit_str, progress)
        } else {
            (if is_buried { "buried".to_string() } else { "-".to_string() }, "-".to_string(), "-".to_string())
        };
        let color = if is_buried { Color::DarkGray } else if fork.active { Color::Green } else {
            match status.as_str() { "started" => Color::Yellow, "locked_in" => Color::Cyan, "defined" => Color::DarkGray, "failed" => Color::Red, _ => Color::White }
        };
        Row::new(vec![(*name).clone(), fork.fork_type.clone(), active_str.to_string(), height_str, status.clone(), bit, progress])
            .style(Style::default().fg(color))
    }).collect();

    let widths = [Constraint::Length(16), Constraint::Length(8), Constraint::Length(7), Constraint::Length(10), Constraint::Length(10), Constraint::Length(4), Constraint::Min(20)];
    let table = Table::new(rows, widths).header(header).block(
        Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)
            .title(format!(" Softforks ({}) ", merged.len()))
            .title_style(Style::default().fg(Color::Cyan).bold()));
    f.render_widget(table, area);
}

fn draw_version_bits(f: &mut Frame, area: Rect, data: &NodeData, selected_bit: u8) {
    if data.recent_block_versions.is_empty() {
        let block = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)
            .title(" Version Bit Signaling (last 144 blocks) ").title_style(Style::default().fg(Color::Yellow).bold());
        f.render_widget(Paragraph::new("No block data").block(block), area);
        return;
    }

    let mut bit_counts: BTreeMap<u8, u64> = BTreeMap::new();
    let total_blocks = data.recent_block_versions.len() as u64;
    for &(_height, version) in &data.recent_block_versions {
        if version >= 0x20000000 {
            for bit in 0..29u8 {
                if version & (1i64 << bit) != 0 { *bit_counts.entry(bit).or_insert(0) += 1; }
            }
        }
    }

    let mut bit_names: BTreeMap<u8, String> = BTreeMap::new();
    let mut bit_descs: BTreeMap<u8, String> = BTreeMap::new();
    bit_names.insert(0, "csv".to_string()); bit_descs.insert(0, "Relative lock-time (BIP68/112/113)".to_string());
    bit_names.insert(1, "segwit".to_string()); bit_descs.insert(1, "Segregated Witness (BIP141/143/147)".to_string());
    bit_names.insert(2, "taproot".to_string()); bit_descs.insert(2, "Taproot/Schnorr (BIP340/341/342)".to_string());
    bit_names.insert(4, "reduced_data".to_string()); bit_descs.insert(4, "Reduced Data Temporary Softfork (BIP110)".to_string());
    for bit in [3u8, 5, 6, 7, 8, 9, 10, 11, 12] { bit_descs.entry(bit).or_insert_with(|| "Unassigned signaling bit".to_string()); }
    for bit in 13..=28u8 { bit_descs.insert(bit, "BIP320 nonce rolling (ASICBoost)".to_string()); }
    for (name, fork) in &data.softforks {
        if let Some(ref bip9) = fork.bip9 { if let Some(bit) = bip9.bit { bit_names.insert(bit, name.clone()); } }
    }

    let header = Row::new(vec!["Bit", "Name", "Signaling", "Pct", "Description"])
        .style(Style::default().fg(Color::Cyan).bold()).bottom_margin(0);

    let rows: Vec<Row> = (0..29u8).map(|bit| {
        let count = bit_counts.get(&bit).copied().unwrap_or(0);
        let is_bip320 = bit >= 13 && bit <= 28;
        let name = bit_names.get(&bit).cloned().unwrap_or_default();
        let desc = bit_descs.get(&bit).cloned().unwrap_or_default();
        let pct = (count as f64 / total_blocks as f64) * 100.0;
        let color = if is_bip320 { Color::DarkGray } else if pct >= 95.0 { Color::Green } else if pct >= 50.0 { Color::Yellow } else { Color::White };
        let marker = if bit == selected_bit { ">" } else { " " };
        Row::new(vec![format!("{} {:>2}", marker, bit), name, format!("{}/{}", format_number(count), format_number(total_blocks)), format!("{:.1}%", pct), desc])
            .style(Style::default().fg(color))
    }).collect();

    let widths = [Constraint::Length(5), Constraint::Length(22), Constraint::Length(12), Constraint::Length(8), Constraint::Min(20)];
    let bip9_blocks = data.recent_block_versions.iter().filter(|&&(_, v)| v >= 0x20000000).count() as u64;
    let table = Table::new(rows, widths).header(header).block(
        Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)
            .title(format!(" Version Bit Signaling (last {} blocks, {} BIP9-versioned) [j/k select, Enter: details] ", total_blocks, bip9_blocks))
            .title_style(Style::default().fg(Color::Yellow).bold()))
        .row_highlight_style(Style::default().bg(Color::Rgb(40, 40, 60)));
    let mut tstate = TableState::default().with_selected(selected_bit as usize);
    f.render_stateful_widget(table, area, &mut tstate);
}

fn bit_detail(bit: u8) -> (&'static str, &'static str) {
    match bit {
        0 => ("BIP9 Bit 0: csv (BIP68/112/113)", "Relative lock-time using sequence numbers. Three BIPs activated together:\n\nBIP68 - Redefines nSequence field to encode relative lock-time,\n  allowing transactions to be time-locked relative to the block\n  that confirmed the parent output (not an absolute block height).\n\nBIP112 - Adds OP_CHECKSEQUENCEVERIFY opcode so scripts can enforce\n  that an output cannot be spent until a relative time has passed.\n  Essential for payment channels and Lightning Network HTLCs.\n\nBIP113 - Uses median-time-past (MTP) instead of block timestamp\n  for time-based lock evaluation, preventing miner manipulation.\n\nActivated at block 419,328 (July 2016). Threshold: 95%."),
        1 => ("BIP9 Bit 1: segwit (BIP141/143/147)", "Segregated Witness - the largest protocol upgrade to Bitcoin.\n\nBIP141 - Moves signature data (witness) outside the base block,\n  creating a new weight-based block limit of 4M weight units.\n  Fixes transaction malleability by excluding witness from txid.\n\nBIP143 - New signature hash algorithm for SegWit inputs that\n  includes the input value, preventing signing-time attacks\n  and enabling efficient hardware wallet verification.\n\nBIP147 - Fixes a dummy stack element malleability in CHECKMULTISIG\n  by requiring it to be exactly OP_0 (null dummy).\n\nActivated at block 481,824 (August 2017). Threshold: 95%.\nEnabled Lightning Network, reduced fees, fixed tx malleability."),
        2 => ("BIP9 Bit 2: taproot (BIP340/341/342)", "Taproot - Schnorr signatures and advanced scripting.\n\nBIP340 - Schnorr signature scheme: more efficient than ECDSA,\n  enables key and signature aggregation (MuSig2), and provides\n  batch verification for faster block validation.\n\nBIP341 - Taproot output structure using a tweaked public key.\n  The common spend path (key path) looks like a regular payment,\n  improving privacy. Complex scripts are hidden in a Merkle tree\n  and only revealed if the key path isn't used.\n\nBIP342 - Tapscript: updated Script rules for Taproot, adds\n  OP_CHECKSIGADD for flexible multisig, removes OP_CHECKMULTISIG,\n  and uses Schnorr-only signature checking in scripts.\n\nActivated at block 709,632 (November 2021). Speedy Trial method."),
        4 => ("BIP9 Bit 4: reduced_data (BIP110)", "Reduced Data Temporary Soft Fork - limits certain transaction\ndata to reduce node resource consumption.\n\nBIP110 proposes temporary restrictions on data-carrying\ntransactions (like OP_RETURN outputs and witness data) to\nreduce blockchain bloat and state growth.\n\nThis is specific to Bitcoin Knots and is not activated on\nBitcoin Core. It aims to address concerns about non-financial\ndata stored on-chain consuming node resources.\n\nStatus: Defined/proposed. Not yet widely signaled."),
        3 | 5..=12 => ("Unassigned BIP9 Signaling Bit", "This bit is currently unassigned and available for future\nsoft fork proposals using the BIP9 signaling mechanism.\n\nBIP9 reserves bits 0-28 for miners to signal readiness for\nconsensus rule changes. Each proposed soft fork is assigned\na specific bit during its signaling period. Once a deployment\nsucceeds or times out, the bit is freed for reuse.\n\nIf blocks are signaling this bit, it may indicate:\n- A new proposal not yet recognized by this software\n- Testing or experimental signaling\n- Random noise (unlikely for low bits)"),
        13..=28 => ("BIP320: Version Rolling for ASICBoost", "This bit is reserved for miner nonce rolling under BIP320.\n\nModern ASIC miners use a technique called ASICBoost that\nmanipulates the block header to gain mining efficiency.\nBIP320 designates bits 13-28 of the version field as\ngeneral-purpose bits that miners can freely toggle as\nadditional nonce space.\n\nThe ~50% signaling rate on each of these bits is expected:\nminers cycle through these bits randomly while hashing,\nso each bit is set roughly half the time by chance.\n\nThese bits are NOT used for soft fork signaling and should\nbe ignored when evaluating protocol upgrade readiness."),
        _ => ("Unknown Bit", "No information available for this bit."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_detail_csv() { let (t, _) = bit_detail(0); assert!(t.contains("csv")); }
    #[test]
    fn bit_detail_segwit() { let (t, _) = bit_detail(1); assert!(t.contains("segwit")); }
    #[test]
    fn bit_detail_taproot() { let (t, _) = bit_detail(2); assert!(t.contains("taproot")); }
    #[test]
    fn bit_detail_bip110() { let (t, _) = bit_detail(4); assert!(t.contains("BIP110")); }
    #[test]
    fn bit_detail_unassigned() { let (t, _) = bit_detail(3); assert!(t.contains("Unassigned")); }
    #[test]
    fn bit_detail_asicboost() { let (t, _) = bit_detail(13); assert!(t.contains("ASICBoost")); let (t2, _) = bit_detail(28); assert!(t2.contains("ASICBoost")); }
}
