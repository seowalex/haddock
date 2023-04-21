# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.7] - 2023-03-31

### Fixed

- Let container device be optional.

## [0.1.6] - 2023-02-15

### Fixed

- Do not create directories with `--dry-run`.

## [0.1.5] - 2023-02-12

### Fixed

- Do not fail if bind mount directory cannot be created.

## [0.1.4] - 2023-02-12

### Fixed

- Make `stop_grace_period` work.
- Ensure that bind mounts that do not exist are created.

## [0.1.3] - 2023-01-18

### Fixed

- Add default network aliases.

## [0.1.2] - 2023-01-18

### Fixed

- Properly fix `--entrypoint` and `--health-cmd` arguments.

## [0.1.1] - 2023-01-18

### Fixed

- Fix `--entrypoint` and `--health-cmd` arguments.
- Hide extra output on `up` command.

## [0.1.0] - 2023-01-18

### Added

- `docker compose` flags.
- `up` command.
- `down` command.
- `create` command.
- `rm` command.
- `start` command.
- `stop` command.
- `restart` command.
- `kill` command.
- `pause` command.
- `unpause` command.
- `run` command.
- `exec` command.
- `cp` command.
- `events` command.
- `logs` command.
- `ps` command.
- `top` command.
- `port` command.
- `ls` command.
- `convert` command.
- `version` command.
- `help` command.
- Nice progress indicators.
- Compose file (de)serialisation.
- Compose file value interpolation.
