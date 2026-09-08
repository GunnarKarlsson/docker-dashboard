I'll look at your existing Android dashboard first so the Docker proposal matches its layout and style.Your Android dashboard’s panel model is clear. I’ll pull the screenshot and the Docker/ECR command surface next so the proposal stays grounded in what CLI and APIs can actually feed.Your Android dash is a good template: one window, a selector panel that drives everything else, live gauges, a main stream, a filtered error stream, and a short insight pane. Treat Docker the same way — except the “device” is a host context, and images exist in three places that you want to compare side by side.

Design principle

One selected context (Local / SSH host / EC2 compose host) drives host stats, containers, local/remote images, volumes, networks, logs, and compose.

One selected image identity (repo + tag or digest) drives tag lists, ECR metadata, and “where does this image exist?”.

One selected container or compose service drives logs, inspect, ports, mounts, and stats.

Keep the first version read-heavy. Footer buttons for Refresh / Stream / Timestamp, same as the Android app. Destructive actions (prune, rm, down -v) can wait or sit behind an explicit confirm.

Local testdata

For dashboard development, do not wait on a real EC2 stack. A fast Alpine Compose project lives at testdata/mock-stack/ (project name dd-mock): api (healthy + mixed logs, port 18080), worker (crash/restart), db (volume), unhealthy (failing healthcheck). No compilers in those images.

```bash
docker compose -f testdata/mock-stack/docker-compose.yml up -d --build
```

Parser samples (docker version JSON, etc.) also belong under testdata/.

Suggested crate split, matching your current layout:

docker-client — wrap docker … locally and ssh host docker …
compose-client — wrap docker compose -f … on the EC2 box
ecr-client — AWS ECR API / aws ecr …
docker-terminal — egui window

Top-left: Contexts (your Devices panel)

List configured targets, not discovered USB gadgets:

| Context | How you talk to it |
|---|---|
| Local | docker on PATH |
| Remote SSH | ssh user@host docker … |
| EC2 compose | same SSH, plus a known compose project dir |

Show per context:

name, host, user
reachability (green / last error)
Docker version (docker version --format '{{.Server.Version}}')
compose project path (EC2 only)
last refresh age

Footer: Refresh. Unreachable hosts listed but not selectable, same as offline adb devices.

That selection is the single source of truth for almost every other panel.

Host health (your RAM + Storage donuts)

Two compact gauges for the selected Docker host:

Disk (Docker) from docker system df  
Images / Containers / Volumes / Build cache — total, active, size, reclaimable.

Host RAM / load from the machine, not the daemon:

free -h
uptime
df -h /

On the remote/EC2 box those run over SSH. Optional third sparkline later from docker stats --no-stream.

A Storage Details sibling panel can dump docker system df -v: largest images, unused images, volume sizes, build cache. That is the Docker equivalent of your per-app storage list.

Images: three panels, one mental model

Do not merge local, remote, and ECR into one table. They are different sources. Selection should still correlate them (same repo:tag highlighted across panels).

1. Local Images

docker images --format json or --format '{{json .}}'

Columns: Repository, Tag, ID (short), Created, Size, Dangling?, In-use count.

Useful extras you can derive:

used by which containers (docker ps -a --filter ancestor=…)
dangling :
local tags for the same ID

2. Remote Images (SSH host / EC2)

Same command over SSH. Same columns. This is what is actually deployable on that machine.

Footer filters: dangling only / unused only / matching selected ECR image.

3. AWS ECR

Not docker images. Use the registry API:

aws ecr describe-repositories
aws ecr describe-images --repository-name …
aws ecr list-images --repository-name … --filter tagStatus=UNTAGGED
aws ecr describe-image-scan-findings (if scanning is on)
aws ecr get-lifecycle-policy (optional, read-only)

Per repository: name, URI, created, tag mutability, scan-on-push, encryption.

Per image: tags, digest, pushed at, size (ECR compressed size ≠ local docker images size), last recorded pull (if available), scan status / severity counts.

A small Tag map strip under the selected image is worth its own panel or a split in this one:

myapp
  latest      → sha256:abc…
  v1.4.2      → sha256:abc…   (same digest)
  v1.4.1      → sha256:def…
    → sha256:fff…

That answers “what is latest really?” better than a flat image list.

Cross-source status chip

When an image is selected, a one-line status is extremely useful:

local: present  |  ec2: present (same digest)  |  ecr: present  |  digest match: yes

That is the whole point of putting all three sources in one window.

Containers (running / stopped / all)

docker ps -a --format '{{json .}}'

Columns: State color, Name, Image, Status (Up 3h / Exited (1) 2m), Ports, Created, ID, Compose project/service labels if present.

com.docker.compose.project / com.docker.compose.service let you group by stack without being in the Compose panel.

Filters in the panel header, like logcat filters:

running / paused / exited / created / restarting
name / image substring
compose project

Selecting a row drives Logs, Inspect, Stats, and (if labeled) the Compose service highlight.

Optional compact Stats table under it or as its own panel:

docker stats --no-stream --format '{{json .}}'

CPU %, Mem usage/limit, Net I/O, Block I/O, PIDs. This is your Network Activity analog, per container instead of per NIC.

Logs (your Logcat)

docker logs --tail 200 -f --timestamps   
or on compose: docker compose logs -f --timestamps --tail 200 [service]

Same UX you already have:

substring filter
service / container filter chips
color by stream (stdout vs stderr) and by log level if the line looks like ERROR / WARN / INFO
Footer Stream / Timestamp
cap line buffer (you already do 10k on logcat)

Logs Errors

Same feed, keep only error/fatal/panic/oom-kill/unhealthy lines. This panel is high value on a compose host because one crashing sidecar is easy to miss in a mixed stream.

Insight (optional, same as Android)

Cluster recent error lines by container + message shape, send a snapshot to the LLM, show HEALTHY / DEGRADING / FAILING plus next checks. Works especially well on compose stacks (api restart loop + postgres connection refused).

What to show in Compose (the question you flagged)

Treat Compose as a project, not as another container list. On the EC2 host you already know the directory (or discover it).

Discovery / status:

docker compose ls
docker compose -f /path/docker-compose.yml ps -a --format json
docker compose config --services
docker compose images
docker compose top          # optional

Recommended Compose panel contents:

Header
project name
compose file path
working dir
project status: running / partial / stopped
profile(s) if any
last up / pull time if you track it

Service table (the main body)

| Service | Image (tag + short digest) | Desired | Current | Health | Restarts | Ports | Depends on |
|---|---|---|---|---|---|---|---|
| api | ecr/…/api:1.4.2 abc123 | 1 | running | healthy | 0 | 8080→8080 | db |
| worker | ecr/…/worker:1.4.1 | 1 | exited (1) | — | 4 | — | redis |
| db | postgres:16 | 1 | running | healthy | 0 | 5432 | — |

Health comes from docker inspect Health.Status. Restarts from inspect RestartCount. Desired vs current is the first thing you want when a stack is “up” but a service is missing.

Secondary Compose views (tabs or stacked sub-panels, keep them small):

File / resolved config — docker compose config (resolved YAML). Show services, images, env keys (not secret values), volumes, networks. This is documentation of what should be running.
Image drift — for each service image, compare  
   compose tag → ECR digest for that tag → digest on the EC2 host → digest running in the container.  
   Chip: in sync / host stale / tag moved.
Volumes used by the project — docker volume ls filtered by project label, plus size from docker system df -v.
Networks — docker network ls + which services are attached.
Project logs — all services, color by service name. Selecting a service narrows the main Logs panel.

I would not dump the raw compose YAML as the primary view. Operators stare at service health and image drift. YAML is the “why is it configured this way” drill-in.

If you add actions later: Up / Down / Restart service / Pull. Not required for a useful v1.

Inspect / detail pane (new vs Android, but you need it)

When a container or image is selected, a JSON-lite detail panel beats another table.

From docker inspect:

image digest actually running (not just the tag)
command / entrypoint
env keys (mask values that look like secrets)
mounts (source → dest, rw/ro)
ports
restart policy
networks + IP
healthcheck + last output
started at / finished at / exit code
labels (compose project, service, working dir)

This is the replacement for hopping into a terminal for docker inspect.

Other panels worth including (still CLI-backed)

Volumes — docker volume ls + docker volume inspect + sizes from system df -v. Show dangling volumes. On EC2 this is usually databases.

Networks — docker network ls + inspect containers attached. Useful when compose services cannot talk.

Events — docker events --since 30m. Compact feed: pull, create, start, die, oom, health_status. Good “what just happened?” panel, analogous to a thin system log.

Build cache / prune preview (read-only) — reclaimable bytes from system df. A “would free X” line is enough; do not auto-prune.

ECR repos overview — if you have many repos, a left list of repositories with image count, last push, scan status. Selecting a repo fills the ECR images panel.

Suggested single-window layout (mirror of your screenshot)

[ Contexts          ] [ Containers                 ] [ Logs                ]
 Disk donut  [                            ] [                     ]
[ Host / Docker df  ] [ Compose services           ] [ Log Errors          ]
[ Local images      ] [ Remote images (EC2)        ] [ Insight             ]
[ ECR repos/images  ] [ Inspect / tags / drift     ] [ Stats or Events     ]

Contexts + gauges stay top-left, like Devices / RAM / Storage.  
Logs + Errors + Insight stay right column, like Logcat.  
The middle is the Docker-specific stuff: containers and compose.  
Bottom row is the three image worlds plus a correlation/inspect pane.

Panels resize by dragging gaps, same as now.

Data sources, mapped cleanly

| Panel | Local | Remote / EC2 | ECR |
|---|---|---|---|
| Contexts | docker version, docker info | same over SSH | aws sts get-caller-identity, describe-registry |
| Disk | docker system df[-v] | same over SSH | repo/image sizes from describe-images |
| Images | docker images | ssh … docker images | describe-repositories, describe-images, list-images |
| Tags | docker image inspect RepoTags / RepoDigests | same | image tags + digest |
| Containers | docker ps -a | same | — |
| Stats | docker stats --no-stream | same | — |
| Logs | docker logs -f | same | — |
| Compose | optional local compose | compose ls/ps/images/config/logs on EC2 | compare service image to ECR digest |
| Events | docker events | same | CloudTrail is overkill; skip |
| Scans | — | — | describe-image-scan-findings |

Transport: local std::process, remote ssh -o BatchMode=yes host -- docker … --format json. Prefer --format json / {{json .}} everywhere so parsing stays boring.

What I would not put in v1

A full terminal / docker exec shell (scope explosion)
Swarm / Kubernetes
Live attach
Automatic prune / delete from the UI
Rendering secret env values
Trying to unify ECR compressed size with local uncompressed size without labeling the difference

Practical default for your stated setup

If the real workflow is “build or tag locally → push ECR → EC2 compose pulls and runs”:

Contexts: Local + that one EC2 host  
ECR repos/images  
Local images + EC2 images  
Compose services on EC2, with digest-drift chips  
Containers on EC2  
Logs + Errors for the selected service  
Host disk/RAM on EC2  
Inspect  

That set already answers the daily questions: what is running, is it the image I think it is, is the host full, and why is it unhealthy.

If you want a next step, I can turn this into a panel-by-panel spec with exact --format strings and a first egui grid matching android-terminal.

----

For the logs panel, it needs to be able to filter out containers/service. So if it starts showing all, then we can filter out some. I suggest the running container appear as toggles at top which can be pressed to include/.exclude individually. Also need filter. Pls update your plan. 

---

this is a Rust project, using egui. It should resemble /Users/gunnar/Projects/rust-projects/android-terminal in achitecture and UI design.