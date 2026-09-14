<div align="center">
  <img src="assets/nude.svg" width="120" alt="nude - a nude-colored square">
  <h1>nude</h1>
  <p><strong>Nu</strong>shell-<strong>D</strong>ock<strong>e</strong>r</p>
  <p><strong>Read-only Docker introspection for Nushell</strong></p>
</div>

---

- [Why?](#why?)
- [So what?](#so-what?)
- [How nude works](#how-nude-works)
- [Installation](#installation)
- [Interface](#interface)
  - [Output modes](#output-modes)
  - [Filtering & completion](#filtering-&-completion)
  - [Dropped flags](#dropped-flags)
- [Commands](#commands)
  - [`nude container diff`](#`nude-container-diff`)
  - [`nude container inspect`](#`nude-container-inspect`)
  - [`nude container ls`](#`nude-container-ls`)
  - [`nude container top`](#`nude-container-top`)
  - [`nude diff`](#`nude-diff`)
  - [`nude history`](#`nude-history`)
  - [`nude image history`](#`nude-image-history`)
  - [`nude image inspect`](#`nude-image-inspect`)
  - [`nude image ls`](#`nude-image-ls`)
  - [`nude images`](#`nude-images`)
  - [`nude info`](#`nude-info`)
  - [`nude network inspect`](#`nude-network-inspect`)
  - [`nude network ls`](#`nude-network-ls`)
  - [`nude plugin inspect`](#`nude-plugin-inspect`)
  - [`nude plugin ls`](#`nude-plugin-ls`)
  - [`nude ps`](#`nude-ps`)
  - [`nude search`](#`nude-search`)
  - [`nude system info`](#`nude-system-info`)
  - [`nude top`](#`nude-top`)
  - [`nude version`](#`nude-version`)
  - [`nude volume inspect`](#`nude-volume-inspect`)
  - [`nude volume ls`](#`nude-volume-ls`)
- [Recipes](#recipes)
    - [Who owns which host port?](#who-owns-which-host-port?)
    - [Trace a mystery process back to its container.](#trace-a-mystery-process-back-to-its-container.)
    - [Audit write access to the host.](#audit-write-access-to-the-host.)
    - [Find containers running stale images.](#find-containers-running-stale-images.)
    - [Where does image disk actually go?](#where-does-image-disk-actually-go?)
    - [Map your compose networks.](#map-your-compose-networks.)

---


## Why?

Inspecting data from the docker cli looks to much like this:
```bash

docker top web | awk 'NR>1 { print $2, $8 }'    # Get each process's PID and command from a container
# Does not actually work
```
I wish I could just `docker top web | select pid command`

## So what?
Nude talks directly with the docker daemon in pure rust, using [bollard](https://crates.io/crates/bollard), to retrieve structured objects and typed data, so we can run things like:
```nu
# Which layers make an image fat?
nude history postgres:16
| where size > 10mb
| sort-by size --reverse
```

- **Nude does not reimplement all of docker.** It covers the introspection commands that benefit the most from structured data.
- **Nude tries to mimick the docker syntax to recreate a familiar environment**. No need to learn a new tool.
- **Nude uses your docker configuration**. No additional setup is required.
- **Nude tries to adhere to docker semantics**, integrating it with richer data.

## How nude works

1. Connects to the Docker daemon over its local socket
2. Performs the request against the Engine API through bollard
3. Converts the raw payload into typed Nushell values (datetimes, filesizes, booleans, nested tables)
4. Shapes the result to the requested output mode (`compact` | `wide` | `full`)

## Installation

Build from source:
```nu
git clone git@github.com:lassoColombo/nude.git
cd nude
cargo build --release

plugin add target/release/nu_plugin_nude
plugin use nude

nude container ls   # verify
```

## Interface


Nude aims to provide a familiar interface so that you don't need to learn a new tool. It exposes a subset of the official docker-cli's commands attempting to retain the original interface.  

However nude is still a nushell plugin that aims to stay idiomatic and ergonomic. For this reason some bashisms have been replaced with a more idiomatic interface, and some flags entirely dropped.

Following, the main conceptual differences you will find from the original docker commands

### Output modes

Every nude command has the extra flag `--output` (`-o`), controlling the density of the returned object:

| Mode | What you get |
| --- | --- |
| `compact` | Flat rows of **primitive** cells only - built to `where`/`sort-by`/`select`. **Default when listing.** |
| `wide` | Richer; keeps nested lists/records (ports, mounts, IPAM, …). **Default for a single object.** |
| `full` | The raw daemon payload, converted verbatim. |

```nu
nude container ls                      # compact table   (list default)
nude container ls -o wide              # + ports / mounts / networks / …
nude container inspect redis           # wide record     (single-object default)
nude container inspect redis -o full   # the raw daemon payload
```

- `compact` is the default for multi-object commands (`nude container ls`)
- `wide` is the default for single-object commands (`nude container inspect web`)

### Filtering & completion

Docker's repeatable `-f status=running -f health=healthy` can't be expressed in Nushell.
Nude exposes instead **each filter key as its own typed, tab-completing flag**:

```nu
nude container ls --status running --network bridge --ancestor nginx
nude network ls --scope local --driver bridge
nude search postgres --official --stars 100
nude image ls --dangling
```

The only exception to this is `--labels`, which is a truly repeatable flag, and is for the moment represented as a comma-separated list of strings, supporting autocompletion:
```nu
nude image ls --labels app=web,tier=db
```

### Dropped flags

Docker's CLI exposes a whole class of flags exists to reshape the returned text: `--format`, `-q`, `--no-trunc`, `-n`, `--last`, `--digests`.

nude hands back **typed, structured values**, and shaping the result in nushell is handy enough:
```nu
nude images | get id                              # docker images -q
nude ps | get name                                # docker ps --format '{{.Names}}'
nude ps -a | sort-by created -r| first 3  # docker ps -n 3
nude images -o wide | get repo_digests | flatten  # docker images --digests
```

<!-- commands-section:start -->
## Commands

| Command                                             | Signature           | Description                                                   |
| --------------------------------------------------- | ------------------- | ------------------------------------------------------------- |
| [`nude container diff`](#nude-container-diff)       | `nothing -> table`  | Filesystem changes to a container since it started            |
| [`nude container inspect`](#nude-container-inspect) | `nothing -> record` | Inspect a container by name/ID                                |
| [`nude container ls`](#nude-container-ls)           | `nothing -> table`  | List containers                                               |
| [`nude container top`](#nude-container-top)         | `nothing -> table`  | Running processes in a container (like `docker top`)          |
| [`nude diff`](#nude-diff)                           | `nothing -> table`  | Filesystem changes to a container (alias of `container diff`) |
| [`nude history`](#nude-history)                     | `nothing -> table`  | Layer history of an image (alias of `image history`)          |
| [`nude image history`](#nude-image-history)         | `nothing -> table`  | Layer history of an image (like `docker history`)             |
| [`nude image inspect`](#nude-image-inspect)         | `nothing -> record` | Inspect an image by name/ID                                   |
| [`nude image ls`](#nude-image-ls)                   | `nothing -> table`  | List images                                                   |
| [`nude images`](#nude-images)                       | `nothing -> table`  | List images (alias of `image ls`)                             |
| [`nude info`](#nude-info)                           | `nothing -> record` | Daemon-wide info (alias of `system info`)                     |
| [`nude network inspect`](#nude-network-inspect)     | `nothing -> record` | Inspect a network by name/ID                                  |
| [`nude network ls`](#nude-network-ls)               | `nothing -> table`  | List networks                                                 |
| [`nude plugin inspect`](#nude-plugin-inspect)       | `nothing -> record` | Inspect a managed plugin by name                              |
| [`nude plugin ls`](#nude-plugin-ls)                 | `nothing -> table`  | List managed plugins                                          |
| [`nude ps`](#nude-ps)                               | `nothing -> table`  | List containers (alias of `container ls`)                     |
| [`nude search`](#nude-search)                       | `nothing -> table`  | Search Docker Hub for images                                  |
| [`nude system info`](#nude-system-info)             | `nothing -> record` | Daemon-wide info: versions, counts, driver, host resources    |
| [`nude top`](#nude-top)                             | `nothing -> table`  | Running processes in a container (alias of `container top`)   |
| [`nude version`](#nude-version)                     | `nothing -> record` | Daemon version: engine, API, Go, OS/arch, and components      |
| [`nude volume inspect`](#nude-volume-inspect)       | `nothing -> record` | Inspect a volume by name                                      |
| [`nude volume ls`](#nude-volume-ls)                 | `nothing -> table`  | List volumes                                                  |

### `nude container diff`

Filesystem changes to a container since it started

**Signature:** `nothing -> table` · **Category:** `docker` · **Type:** `plugin`

**Parameters**

| Parameter | Type     | Description          |
| --------- | -------- | -------------------- |
| `name`    | `string` | Container name or ID |

**Flags**

| Flag             | Type     | Description                                               |
| ---------------- | -------- | --------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact) |

### `nude container inspect`

Inspect a container by name/ID

**Signature:** `nothing -> record` · **Category:** `docker` · **Type:** `plugin`

**Parameters**

| Parameter | Type     | Description          |
| --------- | -------- | -------------------- |
| `name`    | `string` | Container name or ID |

**Flags**

| Flag             | Type     | Description                                                              |
| ---------------- | -------- | ------------------------------------------------------------------------ |
| `--output`, `-o` | `string` | Output format: wide \| full (default: wide)                              |
| `--show-labels`  | `switch` | Enrich the container with a `labels` column (wide; full always has them) |

### `nude container ls`

List containers

**Signature:** `nothing -> table` · **Category:** `docker` · **Type:** `plugin`

**Flags**

| Flag             | Type     | Description                                                                       |
| ---------------- | -------- | --------------------------------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact)                         |
| `--all`, `-a`    | `switch` | Include stopped containers                                                        |
| `--show-labels`  | `switch` | Enrich each container with a `labels` column (compact/wide; full always has them) |
| `--status`       | `string` | State: created\|restarting\|running\|removing\|paused\|exited\|dead               |
| `--health`       | `string` | Health: starting\|healthy\|unhealthy\|none                                        |
| `--exited`       | `int`    | Exit code (matches stopped containers)                                            |
| `--ancestor`     | `string` | Created from this image (name[:tag], id, or digest)                               |
| `--before`       | `string` | Created before this container (name or id)                                        |
| `--since`        | `string` | Created since this container (name or id)                                         |
| `--name`         | `string` | Name substring (use `inspect` for an exact lookup)                                |
| `--id`           | `string` | Container id prefix                                                               |
| `--network`      | `string` | Attached to this network (name or id)                                             |
| `--volume`       | `string` | Uses this volume (name or mount destination)                                      |
| `--publish`      | `string` | Publishes this port (`port[/proto]` or `start-end[/proto]`)                       |
| `--expose`       | `string` | Exposes this port (`port[/proto]` or `start-end[/proto]`)                         |
| `--labels`       | `string` | Labels, comma-separated: `key` or `key=value` (e.g. `a=b,c=d`)                    |
| `--is-task`      | `switch` | Only swarm service tasks                                                          |

### `nude container top`

Running processes in a container (like `docker top`)

**Signature:** `nothing -> table` · **Category:** `docker` · **Type:** `plugin`

**Parameters**

| Parameter | Type     | Description                            |
| --------- | -------- | -------------------------------------- |
| `name`    | `string` | Container name or ID (must be running) |

**Flags**

| Flag             | Type     | Description                                                                       |
| ---------------- | -------- | --------------------------------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact)                         |
| `--ps-args`      | `string` | Arguments for the host's `ps` (default: a typed column set; docker's own is `-ef`) |

### `nude diff`

Filesystem changes to a container (alias of `container diff`)

**Signature:** `nothing -> table` · **Category:** `docker` · **Type:** `plugin`

**Parameters**

| Parameter | Type     | Description          |
| --------- | -------- | -------------------- |
| `name`    | `string` | Container name or ID |

**Flags**

| Flag             | Type     | Description                                               |
| ---------------- | -------- | --------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact) |

### `nude history`

Layer history of an image (alias of `image history`)

**Signature:** `nothing -> table` · **Category:** `docker` · **Type:** `plugin`

**Parameters**

| Parameter | Type     | Description                      |
| --------- | -------- | -------------------------------- |
| `name`    | `string` | Image reference (repo:tag) or ID |

**Flags**

| Flag             | Type     | Description                                               |
| ---------------- | -------- | --------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact) |

### `nude image history`

Layer history of an image (like `docker history`)

**Signature:** `nothing -> table` · **Category:** `docker` · **Type:** `plugin`

**Parameters**

| Parameter | Type     | Description                      |
| --------- | -------- | -------------------------------- |
| `name`    | `string` | Image reference (repo:tag) or ID |

**Flags**

| Flag             | Type     | Description                                               |
| ---------------- | -------- | --------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact) |

### `nude image inspect`

Inspect an image by name/ID

**Signature:** `nothing -> record` · **Category:** `docker` · **Type:** `plugin`

**Parameters**

| Parameter | Type     | Description                      |
| --------- | -------- | -------------------------------- |
| `name`    | `string` | Image reference (repo:tag) or ID |

**Flags**

| Flag             | Type     | Description                                                          |
| ---------------- | -------- | -------------------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: wide \| full (default: wide)                          |
| `--show-labels`  | `switch` | Enrich the image with a `labels` column (wide; full always has them) |

### `nude image ls`

List images

**Signature:** `nothing -> table` · **Category:** `docker` · **Type:** `plugin`

**Flags**

| Flag             | Type     | Description                                                                   |
| ---------------- | -------- | ----------------------------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact)                     |
| `--all`, `-a`    | `switch` | Include intermediate (layer) images                                           |
| `--show-labels`  | `switch` | Enrich each image with a `labels` column (compact/wide; full always has them) |
| `--reference`    | `string` | Reference pattern (name[:tag], e.g. `nginx` or `ngin*`)                       |
| `--before`       | `string` | Created before this image (name or id)                                        |
| `--since`        | `string` | Created since this image (name or id)                                         |
| `--dangling`     | `switch` | Only dangling (untagged) images                                               |
| `--labels`       | `string` | Labels, comma-separated: `key` or `key=value` (e.g. `a=b,c=d`)                |

### `nude images`

List images (alias of `image ls`)

**Signature:** `nothing -> table` · **Category:** `docker` · **Type:** `plugin`

**Flags**

| Flag             | Type     | Description                                                                   |
| ---------------- | -------- | ----------------------------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact)                     |
| `--all`, `-a`    | `switch` | Include intermediate (layer) images                                           |
| `--show-labels`  | `switch` | Enrich each image with a `labels` column (compact/wide; full always has them) |
| `--reference`    | `string` | Reference pattern (name[:tag], e.g. `nginx` or `ngin*`)                       |
| `--before`       | `string` | Created before this image (name or id)                                        |
| `--since`        | `string` | Created since this image (name or id)                                         |
| `--dangling`     | `switch` | Only dangling (untagged) images                                               |
| `--labels`       | `string` | Labels, comma-separated: `key` or `key=value` (e.g. `a=b,c=d`)                |

### `nude info`

Daemon-wide info (alias of `system info`)

**Signature:** `nothing -> record` · **Category:** `docker` · **Type:** `plugin`

**Flags**

| Flag             | Type     | Description                                               |
| ---------------- | -------- | --------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact) |

### `nude network inspect`

Inspect a network by name/ID

**Signature:** `nothing -> record` · **Category:** `docker` · **Type:** `plugin`

**Parameters**

| Parameter | Type     | Description        |
| --------- | -------- | ------------------ |
| `name`    | `string` | Network name or ID |

**Flags**

| Flag             | Type     | Description                                                            |
| ---------------- | -------- | ---------------------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: wide \| full (default: wide)                            |
| `--show-labels`  | `switch` | Enrich the network with a `labels` column (wide; full always has them) |

### `nude network ls`

List networks

**Signature:** `nothing -> table` · **Category:** `docker` · **Type:** `plugin`

**Flags**

| Flag             | Type     | Description                                                                     |
| ---------------- | -------- | ------------------------------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact)                       |
| `--show-labels`  | `switch` | Enrich each network with a `labels` column (compact/wide; full always has them) |
| `--driver`       | `string` | Network driver: bridge\|host\|overlay\|macvlan\|ipvlan\|none                    |
| `--id`           | `string` | Network id prefix                                                               |
| `--name`         | `string` | Name substring (use `inspect` for an exact lookup)                              |
| `--scope`        | `string` | Scope: local\|global\|swarm                                                     |
| `--type`         | `string` | Type: builtin\|custom                                                           |
| `--dangling`     | `switch` | Only networks not used by any container                                         |
| `--labels`       | `string` | Labels, comma-separated: `key` or `key=value` (e.g. `a=b,c=d`)                  |

### `nude plugin inspect`

Inspect a managed plugin by name

**Signature:** `nothing -> record` · **Category:** `docker` · **Type:** `plugin`

**Parameters**

| Parameter | Type     | Description |
| --------- | -------- | ----------- |
| `name`    | `string` | Plugin name |

**Flags**

| Flag             | Type     | Description                                 |
| ---------------- | -------- | ------------------------------------------- |
| `--output`, `-o` | `string` | Output format: wide \| full (default: wide) |

### `nude plugin ls`

List managed plugins

**Signature:** `nothing -> table` · **Category:** `docker` · **Type:** `plugin`

**Flags**

| Flag             | Type     | Description                                                                             |
| ---------------- | -------- | --------------------------------------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact)                               |
| `--capability`   | `string` | Capability: volumedriver\|networkdriver\|ipamdriver\|authz\|logdriver\|metricscollector |
| `--enabled`      | `switch` | Only enabled plugins                                                                    |

### `nude ps`

List containers (alias of `container ls`)

**Signature:** `nothing -> table` · **Category:** `docker` · **Type:** `plugin`

**Flags**

| Flag             | Type     | Description                                                                       |
| ---------------- | -------- | --------------------------------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact)                         |
| `--all`, `-a`    | `switch` | Include stopped containers                                                        |
| `--show-labels`  | `switch` | Enrich each container with a `labels` column (compact/wide; full always has them) |
| `--status`       | `string` | State: created\|restarting\|running\|removing\|paused\|exited\|dead               |
| `--health`       | `string` | Health: starting\|healthy\|unhealthy\|none                                        |
| `--exited`       | `int`    | Exit code (matches stopped containers)                                            |
| `--ancestor`     | `string` | Created from this image (name[:tag], id, or digest)                               |
| `--before`       | `string` | Created before this container (name or id)                                        |
| `--since`        | `string` | Created since this container (name or id)                                         |
| `--name`         | `string` | Name substring (use `inspect` for an exact lookup)                                |
| `--id`           | `string` | Container id prefix                                                               |
| `--network`      | `string` | Attached to this network (name or id)                                             |
| `--volume`       | `string` | Uses this volume (name or mount destination)                                      |
| `--publish`      | `string` | Publishes this port (`port[/proto]` or `start-end[/proto]`)                       |
| `--expose`       | `string` | Exposes this port (`port[/proto]` or `start-end[/proto]`)                         |
| `--labels`       | `string` | Labels, comma-separated: `key` or `key=value` (e.g. `a=b,c=d`)                    |
| `--is-task`      | `switch` | Only swarm service tasks                                                          |

### `nude search`

Search Docker Hub for images

**Signature:** `nothing -> table` · **Category:** `docker` · **Type:** `plugin`

**Parameters**

| Parameter | Type     | Description                                                         |
| --------- | -------- | ------------------------------------------------------------------- |
| `term`    | `string` | Search term (matched against Docker Hub image names & descriptions) |

**Flags**

| Flag             | Type     | Description                                                    |
| ---------------- | -------- | -------------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact)      |
| `--limit`        | `int`    | Maximum number of results (Docker Hub caps at 100; default 25) |
| `--stars`        | `int`    | Only images with at least this many stars                      |
| `--official`     | `switch` | Only official images (disabled-only: `\| where not official`)  |

### `nude system info`

Daemon-wide info: versions, counts, driver, host resources

**Signature:** `nothing -> record` · **Category:** `docker` · **Type:** `plugin`

**Flags**

| Flag             | Type     | Description                                               |
| ---------------- | -------- | --------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact) |

### `nude top`

Running processes in a container (alias of `container top`)

**Signature:** `nothing -> table` · **Category:** `docker` · **Type:** `plugin`

**Parameters**

| Parameter | Type     | Description                            |
| --------- | -------- | -------------------------------------- |
| `name`    | `string` | Container name or ID (must be running) |

**Flags**

| Flag             | Type     | Description                                                                       |
| ---------------- | -------- | --------------------------------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact)                         |
| `--ps-args`      | `string` | Arguments for the host's `ps` (default: a typed column set; docker's own is `-ef`) |

### `nude version`

Daemon version: engine, API, Go, OS/arch, and components

**Signature:** `nothing -> record` · **Category:** `docker` · **Type:** `plugin`

**Flags**

| Flag             | Type     | Description                                               |
| ---------------- | -------- | --------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact) |

### `nude volume inspect`

Inspect a volume by name

**Signature:** `nothing -> record` · **Category:** `docker` · **Type:** `plugin`

**Parameters**

| Parameter | Type     | Description |
| --------- | -------- | ----------- |
| `name`    | `string` | Volume name |

**Flags**

| Flag             | Type     | Description                                                           |
| ---------------- | -------- | --------------------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: wide \| full (default: wide)                           |
| `--show-labels`  | `switch` | Enrich the volume with a `labels` column (wide; full always has them) |

### `nude volume ls`

List volumes

**Signature:** `nothing -> table` · **Category:** `docker` · **Type:** `plugin`

**Flags**

| Flag             | Type     | Description                                                                    |
| ---------------- | -------- | ------------------------------------------------------------------------------ |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact)                      |
| `--show-labels`  | `switch` | Enrich each volume with a `labels` column (compact/wide; full always has them) |
| `--driver`       | `string` | Volume driver (e.g. `local`, or a volume plugin)                               |
| `--name`         | `string` | Name substring (use `inspect` for an exact lookup)                             |
| `--dangling`     | `switch` | Only volumes not used by any container                                         |
| `--labels`       | `string` | Labels, comma-separated: `key` or `key=value` (e.g. `a=b,c=d`)                 |
<!-- commands-section:end -->

## Recipes

#### Who owns which host port?

```nu
nude ps -o wide
| select name ports
| flatten ports --all
| where host_port? != null
| sort-by host_port
```
```
╭───┬────────────────┬────────────────┬───────┬─────────┬───────────╮
│ # │      name      │ container_port │ proto │ host_ip │ host_port │
├───┼────────────────┼────────────────┼───────┼─────────┼───────────┤
│ 0 │ otel-lgtm      │           3000 │ tcp   │ 0.0.0.0 │      3000 │
│ 1 │ dumbo-postgres │           5432 │ tcp   │ 0.0.0.0 │      5432 │
│ 2 │ redis          │           6379 │ tcp   │ 0.0.0.0 │      6379 │
╰───┴────────────────┴────────────────┴───────┴─────────┴───────────╯
```

#### Trace a mystery process back to its container.

```nu
nude ps
| each {|c| nude top $c.name | insert container $c.name }
| flatten
| where pid == 69046
| select container user pid command started cpu_time
```

#### Audit write access to the host.

```nu
nude ps -o wide
| select name mounts
| flatten mounts --all
| where type? == bind and rw? == true
| select name source destination
```

#### Find containers running stale images.

```nu
nude ps
| insert image_created {|c| nude image inspect $c.image | get created }
| where image_created < ((date now) - 60day)
| select name image image_created
```
```
╭───┬─────────────────────┬──────────────────────────┬───────────────╮
│ # │        name         │          image           │ image_created │
├───┼─────────────────────┼──────────────────────────┼───────────────┤
│ 0 │ mole-psql-local-dev │ postgres:16              │ 4 months ago  │
│ 1 │ redis               │ redis/redis-stack:latest │ 8 months ago  │
│ 2 │ dumbo-minio         │ minio/minio:latest       │ 10 months ago │
╰───┴─────────────────────┴──────────────────────────┴───────────────╯
```

#### Where does image disk actually go?

```nu
nude images
| uniq-by id
| group-by repository --to-table
| insert total {|g| $g.items.size | math sum }
| sort-by total --reverse
| select repository total
```
```
╭───┬────────────────────┬────────╮
│ # │     repository     │ total  │
├───┼────────────────────┼────────┤
│ 0 │ texlive/texlive    │ 8.7 GB │
│ 1 │ grafana/otel-lgtm  │ 3.1 GB │
│ 2 │ trinodb/trino      │ 2.4 GB │
╰───┴────────────────────┴────────╯
```


#### Map your compose networks.

```nu
nude network ls --type custom
| each {|n| nude network inspect $n.name | get containers | insert network $n.name }
| flatten
| select network name ipv4
```
```
╭───┬──────────────────────────────────┬─────────────────────────────┬───────────────╮
│ # │             network              │            name             │     ipv4      │
├───┼──────────────────────────────────┼─────────────────────────────┼───────────────┤
│ 0 │ mole-prometheus-devutils_default │ mole-prometheus-local-dev   │ 172.24.0.3/16 │
│ 1 │ mole-prometheus-devutils_default │ mole-prometheus-node-export │ 172.24.0.2/16 │
│ 2 │ devutils_default                 │ redis                       │ 172.18.0.5/16 │
╰───┴──────────────────────────────────┴─────────────────────────────┴───────────────╯
```
