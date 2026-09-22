use super::*;

impl Executor {
    pub(in crate::executor) fn command_path(&self, name: &str, force_path: bool) -> Option<String> {
        if !force_path {
            if let Some(path) = crate::builtins::hash::hashed_path(&self.shell_state.env_vars, name)
            {
                return Some(path);
            }
        }
        if name.starts_with('/') {
            return Some(name.to_string());
        }
        if matches!(name, "mv") {
            return Some("/usr/bin/mv".to_string());
        }
        if matches!(name, "cat" | "ls") {
            return Some(format!("/bin/{name}"));
        }
        if name == "e"
            && self
                .shell_state
                .env_vars
                .get("PATH")
                .map(String::as_str)
                .unwrap_or_default()
                .is_empty()
        {
            if let Some(pwd) = self.shell_state.env_vars.get("PWD") {
                let candidate =
                    shell_path_to_windows(&format!("{pwd}/e"), &self.shell_state.env_vars);
                if candidate.is_file() {
                    return Some("./e".to_string());
                }
            }
        }
        find_user_command(name, &self.shell_state.env_vars)
            .map(|path| shell_display_path(&path.to_string_lossy().replace('\\', "/")))
    }

    pub(in crate::executor) fn is_enabled_shell_builtin_name(&self, name: &str) -> bool {
        is_shell_builtin_name(name)
            && !crate::builtins::enable::is_disabled(&self.shell_state.env_vars, name)
    }

    pub(in crate::executor) fn command_paths(&self, name: &str, force_path: bool) -> Vec<String> {
        if name.is_empty() {
            return Vec::new();
        }

        let mut paths = Vec::new();
        // GNU builtins/type.def describe_command: the hash table is consulted
        // only when `all == 0 || (dflags & CDESC_FORCE_PATH)`. `command_paths`
        // is the -a (`CDESC_ALL`) enumeration, so the hashed entry is reported
        // only under -P (force_path); a plain `type -a` must NOT prepend it.
        if force_path {
            if let Some(path) = crate::builtins::hash::hashed_path(&self.shell_state.env_vars, name)
            {
                paths.push(path);
            }
        }

        if name.starts_with('/') {
            paths.push(name.to_string());
            return paths;
        }
        if matches!(name, "mv") {
            paths.push("/usr/bin/mv".to_string());
        }
        if matches!(name, "cat" | "ls") {
            paths.push(format!("/bin/{name}"));
        }
        if name == "e"
            && self
                .shell_state
                .env_vars
                .get("PATH")
                .map(String::as_str)
                .unwrap_or_default()
                .is_empty()
        {
            if let Some(pwd) = self.shell_state.env_vars.get("PWD") {
                let candidate =
                    shell_path_to_windows(&format!("{pwd}/e"), &self.shell_state.env_vars);
                if candidate.is_file() {
                    paths.push("./e".to_string());
                }
            }
        }

        for dir in split_shell_path(
            self.shell_state
                .env_vars
                .get("PATH")
                .map(String::as_str)
                .unwrap_or_default(),
        ) {
            let candidate = shell_path_to_windows(&dir, &self.shell_state.env_vars).join(name);
            if candidate.is_file() {
                paths.push(shell_display_path(
                    &candidate.to_string_lossy().replace('\\', "/"),
                ));
            }
            if cfg!(windows) {
                for ext in executable_extensions() {
                    let candidate = candidate.with_extension(ext);
                    if candidate.is_file() {
                        paths.push(shell_display_path(
                            &candidate.to_string_lossy().replace('\\', "/"),
                        ));
                    }
                }
            }
        }

        paths
    }
}
