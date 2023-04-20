# haddock

`haddock` aims to be a drop-in replacement for Docker Compose, supporting Podman 4.3.0 and above. All Docker Compose commands are implemented except `build`, `pull` and `push`.

```
Docker Compose for Podman

Usage: haddock [OPTIONS] <COMMAND>

Commands:
  convert  Converts the Compose file to platform's canonical format
  cp       Copy files/folders between a service container and the local filesystem
  create   Creates containers for a service
  down     Stop and remove containers, networks
  events   Receive real time events from containers
  exec     Execute a command in a running container
  help     Print this message or the help of the given subcommand(s)
  kill     Force stop service containers
  logs     View output from containers
  ls       List running Compose projects
  pause    Pause services
  port     Print the public port for a port binding
  ps       List containers
  restart  Restart service containers
  rm       Removes stopped service containers
  run      Run a one-off command on a service
  start    Start services
  stop     Stop services
  top      Display the running processes
  unpause  Unpause services
  up       Create and start containers
  version  Print version

Options:
      --dry-run                                Only show the Podman commands that will be executed
      --env-file <ENV_FILE>                    Specify an alternate environment file
  -f, --file <FILE>                            Compose configuration files
  -h, --help                                   Print help
  -p, --project-name <PROJECT_NAME>            Project name
      --profile <PROFILE>                      Specify a profile to enable
      --project-directory <PROJECT_DIRECTORY>  Specify an alternate working directory
  -V, --version                                Print version
```

## Installation

Install using `cargo`:

```
cargo install haddock
```
