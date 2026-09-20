path = 'd:/DT_Works/Free Tools/Pilot/crates/port-manager/src/lib.rs'
with open(path, 'r', encoding='utf-8-sig') as f:
    c = f.read()

# Fix the collapsible if statements
old = '''            for line in stdout.lines() {
            if line.contains(&format!(":{}", port)) && (line.contains("TCP") || line.contains("UDP")) {
                if let Some(caps) = pid_regex.captures(line) {
                    if let Ok(pid) = caps.get(1).unwrap().as_str().parse::<u32>() {
                        if let Some(owner) = self.get_process_info_windows(pid) {
                            return Some(owner);
                        }
                    }
                }
            }
        }'''

new = '''            for line in stdout.lines() {
            if line.contains(&format!(":{}", port)) && (line.contains("TCP") || line.contains("UDP"))
                && let Some(caps) = pid_regex.captures(line)
                && let Ok(pid) = caps.get(1).unwrap().as_str().parse::<u32>()
                && let Some(owner) = self.get_process_info_windows(pid)
            {
                return Some(owner);
            }
        }'''

c = c.replace(old, new)
with open(path, 'w', encoding='utf-8') as f:
    f.write(c)
print('Done')