use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SyscallAction {
    Allow,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyscallRule {
    pub name: String,
    pub action: SyscallAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub syscall_rules: Vec<SyscallRule>,
    pub max_memory: Option<u64>,
    pub max_cpu_time: Option<u64>,
    pub allow_networking: bool,
    pub allow_filesystem: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            syscall_rules: Vec::new(),
            max_memory: None,
            max_cpu_time: None,
            allow_networking: false,
            allow_filesystem: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PolicyBuilder {
    syscall_rules: Vec<SyscallRule>,
    max_memory: Option<u64>,
    max_cpu_time: Option<u64>,
    allow_networking: bool,
    allow_filesystem: bool,
}

impl Default for PolicyBuilder {
    fn default() -> Self {
        Self {
            syscall_rules: Vec::new(),
            max_memory: None,
            max_cpu_time: None,
            allow_networking: false,
            allow_filesystem: false,
        }
    }
}

impl PolicyBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn allow_syscall(mut self, name: &str) -> Self {
        self.syscall_rules.push(SyscallRule {
            name: name.to_string(),
            action: SyscallAction::Allow,
        });
        self
    }

    pub fn deny_syscall(mut self, name: &str) -> Self {
        self.syscall_rules.push(SyscallRule {
            name: name.to_string(),
            action: SyscallAction::Deny,
        });
        self
    }

    pub fn max_memory(mut self, bytes: u64) -> Self {
        self.max_memory = Some(bytes);
        self
    }

    pub fn max_cpu_time(mut self, ms: u64) -> Self {
        self.max_cpu_time = Some(ms);
        self
    }

    pub fn allow_networking(mut self, allow: bool) -> Self {
        self.allow_networking = allow;
        self
    }

    pub fn allow_filesystem(mut self, allow: bool) -> Self {
        self.allow_filesystem = allow;
        self
    }

    pub fn build(self) -> Policy {
        Policy {
            syscall_rules: self.syscall_rules,
            max_memory: self.max_memory,
            max_cpu_time: self.max_cpu_time,
            allow_networking: self.allow_networking,
            allow_filesystem: self.allow_filesystem,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_is_restrictive() {
        let p = Policy::default();
        assert!(!p.allow_networking);
        assert!(!p.allow_filesystem);
        assert!(p.max_memory.is_none());
        assert!(p.max_cpu_time.is_none());
        assert!(p.syscall_rules.is_empty());
    }

    #[test]
    fn builder_constructs_policy() {
        let p = PolicyBuilder::new()
            .allow_syscall("read")
            .allow_syscall("write")
            .deny_syscall("fork")
            .max_memory(256 * 1024 * 1024)
            .max_cpu_time(5000)
            .allow_networking(true)
            .allow_filesystem(false)
            .build();

        assert_eq!(p.syscall_rules.len(), 3);
        assert_eq!(p.max_memory, Some(256 * 1024 * 1024));
        assert_eq!(p.max_cpu_time, Some(5000));
        assert!(p.allow_networking);
        assert!(!p.allow_filesystem);

        assert_eq!(p.syscall_rules[0].name, "read");
        assert_eq!(p.syscall_rules[0].action, SyscallAction::Allow);
        assert_eq!(p.syscall_rules[2].name, "fork");
        assert_eq!(p.syscall_rules[2].action, SyscallAction::Deny);
    }

    #[test]
    fn allow_deny_rules_ordering() {
        let p = PolicyBuilder::new()
            .deny_syscall("execve")
            .allow_syscall("read")
            .build();

        assert_eq!(p.syscall_rules.len(), 2);
        assert!(matches!(p.syscall_rules[0].action, SyscallAction::Deny));
        assert!(matches!(p.syscall_rules[1].action, SyscallAction::Allow));
    }

    #[test]
    fn builder_chaining_produces_correct_policy() {
        let p = PolicyBuilder::new()
            .max_memory(134217728)
            .max_cpu_time(10000)
            .allow_networking(true)
            .allow_syscall("mmap")
            .build();

        assert_eq!(p.max_memory, Some(134217728));
        assert_eq!(p.max_cpu_time, Some(10000));
        assert!(p.allow_networking);
        assert_eq!(p.syscall_rules.len(), 1);
    }

    #[test]
    fn multiple_rules_for_same_syscall() {
        let p = PolicyBuilder::new()
            .allow_syscall("ioctl")
            .deny_syscall("ioctl")
            .build();

        assert_eq!(p.syscall_rules.len(), 2);
    }
}
