//! Remediation hints: which versions fix a vulnerability.

use super::osv_client::OsvVulnerability;
use std::cmp::Ordering;

/// Every `fixed` version OSV lists for `package` in `vuln`, ascending and
/// deduplicated. Git-commit ranges are skipped since their `fixed` values
/// are hashes, not versions.
pub fn fixed_versions(vuln: &OsvVulnerability, package: &str) -> Vec<String> {
    let pypi = is_pypi(vuln, package);
    let mut fixed: Vec<String> = vuln
        .affected
        .iter()
        .filter(|a| {
            a.package
                .as_ref()
                .is_some_and(|p| p.name.eq_ignore_ascii_case(package))
        })
        .flat_map(|a| &a.ranges)
        .filter(|r| r.range_type != "GIT")
        .flat_map(|r| &r.events)
        .filter_map(|e| e.fixed.clone())
        .collect();
    fixed.sort_by(|a, b| compare_for(pypi, a, b));
    fixed.dedup();
    fixed
}

/// The lowest fixed version strictly above `scanned`, i.e. what to
/// upgrade to.
pub fn fix_for(vuln: &OsvVulnerability, package: &str, scanned: &str) -> Option<String> {
    let pypi = is_pypi(vuln, package);
    fixed_versions(vuln, package)
        .into_iter()
        .find(|v| compare_for(pypi, v, scanned) == Ordering::Greater)
}

/// Whether `package` in `vuln` belongs to the PyPI ecosystem, in which
/// case PEP 440 ordering applies.
fn is_pypi(vuln: &OsvVulnerability, package: &str) -> bool {
    vuln.affected.iter().any(|a| {
        a.package.as_ref().is_some_and(|p| {
            p.name.eq_ignore_ascii_case(package) && p.ecosystem.eq_ignore_ascii_case("PyPI")
        })
    })
}

/// Compares two versions using the comparator appropriate for the
/// ecosystem: PEP 440 for PyPI, semver-style otherwise.
fn compare_for(pypi: bool, a: &str, b: &str) -> Ordering {
    if pypi {
        compare_pep440(a, b)
    } else {
        compare_versions(a, b)
    }
}

/// Compares dotted versions numerically; a pre-release suffix (`-rc.1`)
/// sorts before the same release, and non-numeric segments count as 0.
fn compare_versions(a: &str, b: &str) -> Ordering {
    fn split(v: &str) -> (Vec<u64>, bool) {
        let v = v.trim_start_matches('v');
        let (core, pre) = match v.split_once(['-', '+']) {
            Some((core, rest)) => (core, v.contains('-') && !rest.is_empty()),
            None => (v, false),
        };
        (
            core.split('.').map(|p| p.parse().unwrap_or(0)).collect(),
            pre,
        )
    }
    let ((na, pa), (nb, pb)) = (split(a), split(b));
    for i in 0..na.len().max(nb.len()) {
        let ord = na.get(i).unwrap_or(&0).cmp(nb.get(i).unwrap_or(&0));
        if ord != Ordering::Equal {
            return ord;
        }
    }
    pb.cmp(&pa)
}

/// A parsed PEP 440 version, sufficient for ordering.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pep440 {
    epoch: u64,
    release: Vec<u64>,
    /// Pre-release marker and number, e.g. `rc1` -> `(PreKind::Rc, 1)`.
    pre: Option<(PreKind, u64)>,
    post: Option<u64>,
    dev: Option<u64>,
    /// Local version segments; numeric segments compare numerically and
    /// sort above alphanumeric ones.
    local: Vec<LocalSegment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum PreKind {
    A,
    B,
    Rc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LocalSegment {
    Numeric(u64),
    Alpha(String),
}

impl Ord for LocalSegment {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (LocalSegment::Numeric(a), LocalSegment::Numeric(b)) => a.cmp(b),
            (LocalSegment::Alpha(a), LocalSegment::Alpha(b)) => a.cmp(b),
            // Numeric segments sort above alphanumeric ones.
            (LocalSegment::Numeric(_), LocalSegment::Alpha(_)) => Ordering::Greater,
            (LocalSegment::Alpha(_), LocalSegment::Numeric(_)) => Ordering::Less,
        }
    }
}

impl PartialOrd for LocalSegment {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Pep440 {
    /// Parses a PEP 440 version string, returning `None` if it is not a
    /// valid PEP 440 version.
    fn parse(input: &str) -> Option<Self> {
        let s = input.trim();
        if s.is_empty() {
            return None;
        }
        let mut rest = s;

        // Epoch: `N!`.
        let mut epoch = 0;
        if let Some(idx) = rest.find('!') {
            epoch = rest[..idx].parse().ok()?;
            rest = &rest[idx + 1..];
        }

        // Local version: everything after `+`.
        let mut local = Vec::new();
        if let Some(idx) = rest.find('+') {
            let local_str = &rest[idx + 1..];
            rest = &rest[..idx];
            if local_str.is_empty() {
                return None;
            }
            for seg in local_str.split(['.', '-', '_']) {
                if seg.is_empty() {
                    return None;
                }
                if seg.chars().all(|c| c.is_ascii_digit()) {
                    local.push(LocalSegment::Numeric(seg.parse().ok()?));
                } else {
                    local.push(LocalSegment::Alpha(seg.to_ascii_lowercase()));
                }
            }
        }

        // Release segments: leading numeric dotted components.
        let release_end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rest.len());
        let release_str = &rest[..release_end];
        if release_str.is_empty() {
            return None;
        }
        let mut release = Vec::new();
        for part in release_str.split('.') {
            if part.is_empty() {
                return None;
            }
            release.push(part.parse().ok()?);
        }
        rest = &rest[release_end..];

        // Pre-release, post-release and dev-release markers, in any order
        // but each at most once.
        let mut pre = None;
        let mut post = None;
        let mut dev = None;
        let mut tail = rest.to_ascii_lowercase();
        // Normalise separators so `.post1`, `-post1`, `_post1` all work.
        tail = tail.replace(['-', '_'], ".");
        while !tail.is_empty() {
            let tail_trimmed = tail.trim_start_matches('.');
            if tail_trimmed.is_empty() {
                break;
            }
            tail = tail_trimmed.to_string();
            let (kind, consumed) = if let Some(r) = tail.strip_prefix("post") {
                ("post", r)
            } else if let Some(r) = tail.strip_prefix("dev") {
                ("dev", r)
            } else if let Some(r) = tail.strip_prefix("alpha") {
                ("a", r)
            } else if let Some(r) = tail.strip_prefix("beta") {
                ("b", r)
            } else if let Some(r) = tail.strip_prefix("pre") {
                ("rc", r)
            } else if let Some(r) = tail.strip_prefix("preview") {
                ("rc", r)
            } else if let Some(r) = tail.strip_prefix("rc") {
                ("rc", r)
            } else if let Some(r) = tail.strip_prefix('a') {
                ("a", r)
            } else if let Some(r) = tail.strip_prefix('b') {
                ("b", r)
            } else if let Some(r) = tail.strip_prefix('c') {
                ("rc", r)
            } else {
                return None;
            };
            let num_str: String = consumed
                .trim_start_matches('.')
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            let num: u64 = if num_str.is_empty() {
                0
            } else {
                num_str.parse().ok()?
            };
            let after = &consumed.trim_start_matches('.')[num_str.len()..];
            tail = after.to_string();
            match kind {
                "post" => {
                    if post.is_some() {
                        return None;
                    }
                    post = Some(num);
                }
                "dev" => {
                    if dev.is_some() {
                        return None;
                    }
                    dev = Some(num);
                }
                k => {
                    if pre.is_some() {
                        return None;
                    }
                    let pk = match k {
                        "a" => PreKind::A,
                        "b" => PreKind::B,
                        _ => PreKind::Rc,
                    };
                    pre = Some((pk, num));
                }
            }
        }

        Some(Pep440 {
            epoch,
            release,
            pre,
            post,
            dev,
            local,
        })
    }

    /// The release tuple with trailing zeros stripped, for comparison.
    fn trimmed_release(&self) -> &[u64] {
        let mut end = self.release.len();
        while end > 0 && self.release[end - 1] == 0 {
            end -= 1;
        }
        &self.release[..end]
    }
}

impl Ord for Pep440 {
    fn cmp(&self, other: &Self) -> Ordering {
        // Epoch.
        let ord = self.epoch.cmp(&other.epoch);
        if ord != Ordering::Equal {
            return ord;
        }

        // Release, comparing with implicit trailing zeros.
        let (ra, rb) = (self.trimmed_release(), other.trimmed_release());
        for i in 0..ra.len().max(rb.len()) {
            let ord = ra.get(i).unwrap_or(&0).cmp(rb.get(i).unwrap_or(&0));
            if ord != Ordering::Equal {
                return ord;
            }
        }

        // Pre-release: a version without a pre-release sorts above one
        // with a pre-release (given equal release).
        let ord = match (&self.pre, &other.pre) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => a.cmp(b),
        };
        if ord != Ordering::Equal {
            return ord;
        }

        // Post-release: a version without a post-release sorts below one
        // with a post-release.
        let ord = match (&self.post, &other.post) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (Some(a), Some(b)) => a.cmp(b),
        };
        if ord != Ordering::Equal {
            return ord;
        }

        // Dev-release: a version without a dev-release sorts above one
        // with a dev-release.
        let ord = match (&self.dev, &other.dev) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => a.cmp(b),
        };
        if ord != Ordering::Equal {
            return ord;
        }

        // Local version: a version without a local segment sorts below one
        // with a local segment.
        match (self.local.is_empty(), other.local.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => {
                for i in 0..self.local.len().max(other.local.len()) {
                    let ord = match (self.local.get(i), other.local.get(i)) {
                        (Some(a), Some(b)) => a.cmp(b),
                        (Some(_), None) => Ordering::Greater,
                        (None, Some(_)) => Ordering::Less,
                        (None, None) => Ordering::Equal,
                    };
                    if ord != Ordering::Equal {
                        return ord;
                    }
                }
                Ordering::Equal
            }
        }
    }
}

impl PartialOrd for Pep440 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Compares two PyPI versions per PEP 440. Invalid PEP 440 versions fall
/// back to the semver-style comparator so ordering is still total.
fn compare_pep440(a: &str, b: &str) -> Ordering {
    match (Pep440::parse(a), Pep440::parse(b)) {
        (Some(pa), Some(pb)) => pa.cmp(&pb),
        _ => compare_versions(a, b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lodash_advisory() -> OsvVulnerability {
        serde_json::from_str(
            r#"{"id":"GHSA-29mw-wpgm-hmr9","aliases":["CVE-2020-28500"],"affected":[
                {"package":{"name":"lodash","ecosystem":"npm"},
                 "ranges":[{"type":"SEMVER","events":[{"introduced":"0"},{"fixed":"4.17.21"}]}]},
                {"package":{"name":"other","ecosystem":"npm"},
                 "ranges":[{"type":"SEMVER","events":[{"introduced":"0"},{"fixed":"9.9.9"}]}]}]}"#,
        )
        .unwrap()
    }

    #[test]
    fn lodash_4_17_4_is_fixed_in_4_17_21() {
        let v = lodash_advisory();
        assert_eq!(fixed_versions(&v, "lodash"), ["4.17.21"]);
        assert_eq!(fix_for(&v, "lodash", "4.17.4").as_deref(), Some("4.17.21"));
        assert_eq!(fix_for(&v, "lodash", "4.17.21"), None);
    }

    #[test]
    fn picks_lowest_fix_above_scanned_version() {
        let v: OsvVulnerability = serde_json::from_str(
            r#"{"id":"X","affected":[{"package":{"name":"p","ecosystem":"npm"},"ranges":[
                {"type":"SEMVER","events":[{"introduced":"0"},{"fixed":"1.2.5"},
                                           {"introduced":"2.0.0"},{"fixed":"2.0.3"}]},
                {"type":"GIT","events":[{"fixed":"deadbeef"}]}]}]}"#,
        )
        .unwrap();
        assert_eq!(fixed_versions(&v, "p"), ["1.2.5", "2.0.3"]);
        assert_eq!(fix_for(&v, "p", "1.0.0").as_deref(), Some("1.2.5"));
        assert_eq!(fix_for(&v, "p", "1.9.0").as_deref(), Some("2.0.3"));
        assert!(compare_versions("1.10.0", "1.9.0").is_gt());
    }

    #[test]
    fn pep440_pre_release_sorts_before_release() {
        assert!(compare_pep440("2.0rc1", "2.0").is_lt());
        assert!(compare_pep440("2.0a1", "2.0b1").is_lt());
        assert!(compare_pep440("2.0b1", "2.0rc1").is_lt());
        assert!(compare_pep440("2.0rc1", "2.0rc2").is_lt());
        assert!(compare_pep440("2.0.dev1", "2.0a1").is_lt());
    }

    #[test]
    fn pep440_post_release_sorts_after_release() {
        assert!(compare_pep440("1.0.post1", "1.0").is_gt());
        assert!(compare_pep440("1.0.post1", "1.0.post2").is_lt());
        assert!(compare_pep440("1.0", "1.0.post1").is_lt());
    }

    #[test]
    fn pep440_epoch_dominates_release() {
        assert!(compare_pep440("1!1.0", "2.0").is_gt());
        assert!(compare_pep440("1!0.1", "0!9.9").is_gt());
        assert!(compare_pep440("2!1.0", "1!9.9").is_gt());
    }

    #[test]
    fn pep440_trailing_zeros_and_local_versions() {
        assert_eq!(compare_pep440("1.0", "1.0.0"), Ordering::Equal);
        assert!(compare_pep440("1.0+local", "1.0").is_gt());
        assert!(compare_pep440("1.0+1", "1.0+abc").is_gt());
    }

    #[test]
    fn pypi_fixed_versions_use_pep440_ordering() {
        let v: OsvVulnerability = serde_json::from_str(
            r#"{"id":"X","affected":[{"package":{"name":"pkg","ecosystem":"PyPI"},"ranges":[
                {"type":"ECOSYSTEM","events":[{"introduced":"0"},{"fixed":"2.0"},
                                              {"introduced":"2.0"},{"fixed":"2.0.post1"}]}]}]}"#,
        )
        .unwrap();
        assert_eq!(fixed_versions(&v, "pkg"), ["2.0", "2.0.post1"]);
        assert_eq!(fix_for(&v, "pkg", "2.0rc1").as_deref(), Some("2.0"));
        assert_eq!(fix_for(&v, "pkg", "2.0").as_deref(), Some("2.0.post1"));
    }
}
