//! Tiny text-table helper shared by the list-style subcommands.
//! Kept deliberately minimal — the proper rendering target is the
//! TUI; this is just for piped/grep'able CLI output.

/// Print `rows` with `headers` as a column-aligned text table. Cells
/// are truncated to 40 chars to keep terminals readable.
pub fn print_table<const N: usize>(headers: &[&str; N], rows: &[[String; N]]) {
    let mut widths = [0usize; N];
    for (i, h) in headers.iter().enumerate() {
        widths[i] = h.len();
    }
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count().min(40));
        }
    }
    let header_strs: [String; N] = std::array::from_fn(|i| headers[i].to_string());
    let sep_strs: [String; N] = std::array::from_fn(|i| "-".repeat(widths[i]));
    print_row(&header_strs, &widths);
    print_row(&sep_strs, &widths);
    for row in rows {
        print_row(row, &widths);
    }
}

fn print_row<const N: usize>(cells: &[String; N], widths: &[usize; N]) {
    let parts: Vec<String> = cells
        .iter()
        .zip(widths.iter())
        .map(|(c, w)| {
            let truncated: String = c.chars().take(40).collect();
            format!("{:<width$}", truncated, width = *w)
        })
        .collect();
    println!("{}", parts.join("  "));
}
