//! Shell completion generation for bash, zsh, and fish
//!
//! Provides functions to generate shell-specific completion scripts
//! for the project catalog directory changer.

use std::fmt;

/// Supported shell types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellType {
    Bash,
    Zsh,
    Fish,
}

impl ShellType {
    /// Parse shell type from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "bash" => Some(ShellType::Bash),
            "zsh" => Some(ShellType::Zsh),
            "fish" => Some(ShellType::Fish),
            _ => None,
        }
    }
}

impl fmt::Display for ShellType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShellType::Bash => write!(f, "bash"),
            ShellType::Zsh => write!(f, "zsh"),
            ShellType::Fish => write!(f, "fish"),
        }
    }
}

/// Generate shell completion script
pub fn generate_completions(shell: ShellType) -> String {
    match shell {
        ShellType::Bash => generate_bash_completions(),
        ShellType::Zsh => generate_zsh_completions(),
        ShellType::Fish => generate_fish_completions(),
    }
}

fn generate_bash_completions() -> String {
    r#"# Bash completion for dprojc shell integration

# Main directory changer function
dpc-cd() {
    local result
    result=$(dpc shell query "$@" 2>/dev/null | head -n 1)
    if [ -n "$result" ] && [ -d "$result" ]; then
        cd "$result" || return 1
        dpc shell record "$result" &>/dev/null &
        echo "Changed to: $result"
    else
        echo "No matching project found for: $*" >&2
        return 1
    fi
}

# Shorter alias
alias j='dpc-cd'

# Completion function
_dpc_cd_completions() {
    local cur
    cur="${COMP_WORDS[COMP_CWORD]}"

    # Get completions from dpc
    local completions
    completions=$(dpc shell complete "$cur" 2>/dev/null)

    COMPREPLY=( $(compgen -W "$completions" -- "$cur") )
}

# Register completions
complete -F _dpc_cd_completions dpc-cd
complete -F _dpc_cd_completions j

# Interactive directory selector
dpc-select() {
    local result
    result=$(dpc tui 2>/dev/null)
    if [ -n "$result" ] && [ -d "$result" ]; then
        cd "$result" || return 1
        dpc shell record "$result" &>/dev/null &
        echo "Changed to: $result"
    fi
}

alias ji='dpc-select'
"#
    .to_string()
}

fn generate_zsh_completions() -> String {
    r#"# Zsh completion for dprojc shell integration

# Main directory changer function
dpc-cd() {
    local result
    result=$(dpc shell query "$@" 2>/dev/null | head -n 1)
    if [[ -n "$result" ]] && [[ -d "$result" ]]; then
        cd "$result" || return 1
        dpc shell record "$result" &>/dev/null &
        echo "Changed to: $result"
    else
        echo "No matching project found for: $*" >&2
        return 1
    fi
}

# Shorter alias
alias j='dpc-cd'

# Completion function
_dpc_cd_completions() {
    local -a completions
    local cur="${words[CURRENT]}"

    # Get completions from dpc
    completions=(${(f)"$(dpc shell complete "$cur" 2>/dev/null)"})

    _describe 'projects' completions
}

# Register completions
compdef _dpc_cd_completions dpc-cd
compdef _dpc_cd_completions j

# Interactive directory selector
dpc-select() {
    local result
    result=$(dpc tui 2>/dev/null)
    if [[ -n "$result" ]] && [[ -d "$result" ]]; then
        cd "$result" || return 1
        dpc shell record "$result" &>/dev/null &
        echo "Changed to: $result"
    fi
}

alias ji='dpc-select'

# Hook to record directory changes (auto-track project roots)
autoload -U add-zsh-hook
_dpc_record_pwd() {
    # Record if we're in a cataloged project root
    # This runs in the background and exits silently if not a project
    dpc shell record "$PWD" &>/dev/null &
}
add-zsh-hook chpwd _dpc_record_pwd
"#
    .to_string()
}

fn generate_fish_completions() -> String {
    r#"# Fish completion for dprojc shell integration

# Main directory changer function
function dpc-cd
    set -l result (dpc shell query $argv 2>/dev/null | head -n 1)
    if test -n "$result" -a -d "$result"
        cd "$result"; or return 1
        dpc shell record "$result" &>/dev/null &
        echo "Changed to: $result"
    else
        echo "No matching project found for: $argv" >&2
        return 1
    end
end

# Shorter alias
alias j='dpc-cd'

# Completion function
complete -c dpc-cd -f -a '(dpc shell complete (commandline -ct) 2>/dev/null)'
complete -c j -f -a '(dpc shell complete (commandline -ct) 2>/dev/null)'

# Interactive directory selector
function dpc-select
    set -l result (dpc tui 2>/dev/null)
    if test -n "$result" -a -d "$result"
        cd "$result"; or return 1
        dpc shell record "$result" &>/dev/null &
        echo "Changed to: $result"
    end
end

alias ji='dpc-select'

# Hook to record directory changes (auto-track project roots)
function _dpc_record_pwd --on-variable PWD
    # Record if we're in a cataloged project root
    # This runs in the background and exits silently if not a project
    dpc shell record "$PWD" &>/dev/null &
end
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_type_from_str() {
        assert_eq!(ShellType::from_str("bash"), Some(ShellType::Bash));
        assert_eq!(ShellType::from_str("zsh"), Some(ShellType::Zsh));
        assert_eq!(ShellType::from_str("fish"), Some(ShellType::Fish));
        assert_eq!(ShellType::from_str("BASH"), Some(ShellType::Bash));
        assert_eq!(ShellType::from_str("invalid"), None);
    }

    #[test]
    fn test_generate_bash_completions() {
        let script = generate_bash_completions();
        assert!(script.contains("dpc-cd()"));
        assert!(script.contains("_dpc_cd_completions"));
        assert!(script.contains("alias j="));
    }

    #[test]
    fn test_generate_zsh_completions() {
        let script = generate_zsh_completions();
        assert!(script.contains("dpc-cd()"));
        assert!(script.contains("_dpc_cd_completions"));
        assert!(script.contains("compdef"));
    }

    #[test]
    fn test_generate_fish_completions() {
        let script = generate_fish_completions();
        assert!(script.contains("function dpc-cd"));
        assert!(script.contains("complete -c dpc-cd"));
        assert!(script.contains("alias j="));
    }

    #[test]
    fn test_generate_completions() {
        for shell in [ShellType::Bash, ShellType::Zsh, ShellType::Fish] {
            let script = generate_completions(shell);
            assert!(!script.is_empty());
            assert!(script.contains("dpc"));
        }
    }
}
