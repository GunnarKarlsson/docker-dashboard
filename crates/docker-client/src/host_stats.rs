use crate::docker::run_host;
use crate::error::DockerError;
use crate::transport::Transport;

/// Host RAM, load, and root-filesystem usage (not Docker disk).
#[derive(Debug, Clone, PartialEq)]
pub struct HostStats {
    pub ram_used_bytes: u64,
    pub ram_total_bytes: u64,
    pub load_1: f32,
    pub load_5: f32,
    pub load_15: f32,
    pub root_used_bytes: u64,
    pub root_total_bytes: u64,
}

impl HostStats {
    pub fn ram_used_fraction(&self) -> f32 {
        if self.ram_total_bytes == 0 {
            0.0
        } else {
            self.ram_used_bytes as f32 / self.ram_total_bytes as f32
        }
    }
}

pub fn fetch_host_stats(transport: &Transport) -> Result<HostStats, DockerError> {
    match transport {
        Transport::Local => fetch_local_host_stats(),
        Transport::Ssh { .. } => fetch_linux_host_stats(transport),
    }
}

fn fetch_local_host_stats() -> Result<HostStats, DockerError> {
    #[cfg(target_os = "macos")]
    {
        fetch_macos_host_stats()
    }
    #[cfg(not(target_os = "macos"))]
    {
        fetch_linux_host_stats(&Transport::Local)
    }
}

#[cfg(target_os = "macos")]
fn fetch_macos_host_stats() -> Result<HostStats, DockerError> {
    let transport = Transport::Local;
    let memsize = stdout(&run_host(&transport, "sysctl", &["-n", "hw.memsize"])?);
    let total_bytes = memsize
        .trim()
        .parse::<u64>()
        .map_err(|err| DockerError::ParseFailed(format!("hw.memsize: {err}")))?;

    let vm_stat = stdout(&run_host(&transport, "vm_stat", &[])?);
    let ram_used_bytes = parse_vm_stat_used(&vm_stat, total_bytes)?;

    let uptime = stdout(&run_host(&transport, "uptime", &[])?);
    let (load_1, load_5, load_15) = parse_load_averages(&uptime)
        .ok_or_else(|| DockerError::ParseFailed("uptime load averages".to_string()))?;

    let df = stdout(&run_host(&transport, "df", &["-kP", "/"])?);
    let (root_used_bytes, root_total_bytes) = parse_df_kp(&df)?;

    Ok(HostStats {
        ram_used_bytes,
        ram_total_bytes: total_bytes,
        load_1,
        load_5,
        load_15,
        root_used_bytes,
        root_total_bytes,
    })
}

fn fetch_linux_host_stats(transport: &Transport) -> Result<HostStats, DockerError> {
    let meminfo = stdout(&run_host(transport, "cat", &["/proc/meminfo"])?);
    let (ram_used_bytes, ram_total_bytes) = parse_meminfo(&meminfo)?;

    let uptime = stdout(&run_host(transport, "uptime", &[])?);
    let (load_1, load_5, load_15) = parse_load_averages(&uptime)
        .ok_or_else(|| DockerError::ParseFailed("uptime load averages".to_string()))?;

    let df = stdout(&run_host(transport, "df", &["-kP", "/"])?);
    let (root_used_bytes, root_total_bytes) = parse_df_kp(&df)?;

    Ok(HostStats {
        ram_used_bytes,
        ram_total_bytes,
        load_1,
        load_5,
        load_15,
        root_used_bytes,
        root_total_bytes,
    })
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub(crate) fn parse_vm_stat_used(text: &str, total_bytes: u64) -> Result<u64, DockerError> {
    let page_size = parse_vm_page_size(text)?;
    let free = parse_vm_pages(text, "Pages free")?;
    let speculative = parse_vm_pages(text, "Pages speculative").unwrap_or(0);
    let free_bytes = (free + speculative).saturating_mul(page_size);
    Ok(total_bytes.saturating_sub(free_bytes))
}

fn parse_vm_page_size(text: &str) -> Result<u64, DockerError> {
    const NEEDLE: &str = "page size of ";
    let start = text
        .find(NEEDLE)
        .ok_or_else(|| DockerError::ParseFailed("vm_stat page size".to_string()))?
        + NEEDLE.len();
    let rest = &text[start..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits
        .parse()
        .map_err(|_| DockerError::ParseFailed("vm_stat page size".to_string()))
}

fn parse_vm_pages(text: &str, label: &str) -> Result<u64, DockerError> {
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix(label) {
            let digits: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
            return digits
                .parse()
                .map_err(|_| DockerError::ParseFailed(format!("vm_stat {label}")));
        }
    }
    Err(DockerError::ParseFailed(format!("vm_stat missing {label}")))
}

pub(crate) fn parse_load_averages(text: &str) -> Option<(f32, f32, f32)> {
    let rest = text
        .split("load averages:")
        .nth(1)
        .or_else(|| text.split("load average:").nth(1))?;
    let nums: Vec<f32> = rest
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() {
                None
            } else {
                part.parse().ok()
            }
        })
        .collect();
    if nums.len() >= 3 {
        Some((nums[0], nums[1], nums[2]))
    } else {
        None
    }
}

pub(crate) fn parse_df_kp(text: &str) -> Result<(u64, u64), DockerError> {
    for line in text.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 3 {
            continue;
        }
        let total_kib: u64 = cols[1]
            .parse()
            .map_err(|_| DockerError::ParseFailed("df total".to_string()))?;
        let used_kib: u64 = cols[2]
            .parse()
            .map_err(|_| DockerError::ParseFailed("df used".to_string()))?;
        return Ok((
            used_kib.saturating_mul(1024),
            total_kib.saturating_mul(1024),
        ));
    }
    Err(DockerError::ParseFailed(
        "df produced no data rows".to_string(),
    ))
}

pub(crate) fn parse_meminfo(text: &str) -> Result<(u64, u64), DockerError> {
    let total = meminfo_kib(text, "MemTotal")?.saturating_mul(1024);
    let available = meminfo_kib(text, "MemAvailable")?.saturating_mul(1024);
    Ok((total.saturating_sub(available), total))
}

fn meminfo_kib(text: &str, key: &str) -> Result<u64, DockerError> {
    let prefix = format!("{key}:");
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix(&prefix) {
            let digits: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
            return digits
                .parse()
                .map_err(|_| DockerError::ParseFailed(format!("meminfo {key}")));
        }
    }
    Err(DockerError::ParseFailed(format!("meminfo missing {key}")))
}

#[cfg(test)]
mod tests {
    use super::parse_load_averages;

    #[test]
    fn parses_macos_and_linux_load() {
        assert_eq!(
            parse_load_averages("12:52  up 47 days, 3 users, load averages: 7.34 6.57 6.96"),
            Some((7.34, 6.57, 6.96))
        );
        assert_eq!(
            parse_load_averages(
                " 12:52:01 up 4 days,  3:01,  1 user,  load average: 0.12, 0.08, 0.01"
            ),
            Some((0.12, 0.08, 0.01))
        );
    }
}
