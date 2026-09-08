use std::process::Command;

/// How the dashboard talks to a Docker engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transport {
    Local,
    Ssh { user: String, host: String },
}

impl Transport {
    /// Human-readable `docker …` / `ssh … docker …` line for errors and logs.
    pub fn describe_command(&self, args: &[&str]) -> String {
        let docker = format!("docker {}", args.join(" "));
        match self {
            Transport::Local => docker,
            Transport::Ssh { user, host } => {
                format!("ssh -o BatchMode=yes {user}@{host} -- {docker}")
            }
        }
    }

    pub(crate) fn command(&self, args: &[&str]) -> Command {
        match self {
            Transport::Local => {
                let mut command = Command::new("docker");
                command.args(args);
                command
            }
            Transport::Ssh { user, host } => {
                let mut command = Command::new("ssh");
                command.args([
                    "-o",
                    "BatchMode=yes",
                    &format!("{user}@{host}"),
                    "--",
                    "docker",
                ]);
                command.args(args);
                command
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Transport;

    #[test]
    fn describe_local_command() {
        assert_eq!(
            Transport::Local.describe_command(&["version", "--format", "{{json .}}"]),
            "docker version --format {{json .}}"
        );
    }

    #[test]
    fn describe_ssh_command() {
        let transport = Transport::Ssh {
            user: "ec2-user".into(),
            host: "10.0.0.8".into(),
        };
        assert_eq!(
            transport.describe_command(&["ps", "-a"]),
            "ssh -o BatchMode=yes ec2-user@10.0.0.8 -- docker ps -a"
        );
    }
}
