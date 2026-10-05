//! "Did you mean ...?" suggestions for typos.

fn distance(a: &str, b: &str) -> usize {
    // edit distance where swapping two neighbouring letters ("scroe" -> "score") counts as one edit
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=b.len() {
        d[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

/// The candidate closest to `name`, if it's close enough to be a likely typo.
pub fn closest(name: &str, candidates: &[String]) -> Option<String> {
    let limit = (name.chars().count() / 3).clamp(1, 3);
    let lower = name.to_lowercase();
    let mut best: Option<(usize, &String)> = None;
    for c in candidates {
        if c == name {
            continue;
        }
        let d = distance(&lower, &c.to_lowercase());
        if d <= limit && best.map_or(true, |(bd, _)| d < bd) {
            best = Some((d, c));
        }
    }
    best.map(|(_, c)| c.clone())
}
