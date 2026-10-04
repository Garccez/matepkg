# :mate: matepkg :package:

Matepkg is a Linux package manager written in Rust, it is based on how [bananapkg](https://github.com/slackjeff/bananapkg) works, but later on I intend include new features. I'm using it as a learning project, it is not yet complete.

## :star: Requirements (building from source)
* **Rust** >= 1.90.0 <br/>
* **^Cargo** >= 1.90.0
----

## Building from source
Clone the repository and enter it.
```
$ git clone https://github.com/Garccez/matepkg
$ cd matepkg
```
Compile it.
```
$ cargo build --release
```
Or, if you wish to install it
```
# cargo install --path . --root /usr/
```
Remove ``--root /usr/`` if you do not wish to install in your /usr/bin/ folder.

## Binary installation
**Not possible**, binaries are not distributed just yet.

## Repositories and named installation

Create a local sync database from a directory containing `.mtz` packages and their
`.mtz.sha256` files:

```sh
mate repo-add core /srv/matepkg-repo/
mate search vim
mate install vim
```

When multiple repositories provide a package, set `MATEPKG_REPOS` to a
colon-separated priority list (for example `core:testing`). The first matching
repository wins; within that repository the highest version wins.

Named installation resolves dependencies from the local sync database and caches
packages under `/var/lib/matepkg/cache/`. Installation stops on the first failure;
packages installed earlier in the same command are rolled back in reverse order.
Rollback cannot undo arbitrary side effects from package hooks; if rollback itself
fails, the error lists packages whose state must be reviewed manually.
If a filesystem write fails partway through a single file extraction, that
partially written file may not be listed for cleanup and must be reviewed
manually.

## Installation root and hooks

`MATEPKG_ROOT` controls where package files are extracted (default `/`). The
database follows that root at `<root>/var/lib/matepkg`; set `MATEPKG_DB_ROOT`
explicitly to override it:

```sh
MATEPKG_ROOT=/mnt/lfs mate install hello
```

Packages may include `hooks.sh` beside `desc.toml`, defining
`pre_install`, `post_install`, `pre_remove`, `post_remove`, `pre_upgrade` and
`post_upgrade`. The hook file is retained in the package database. Hooks are
skipped with a warning when `MATEPKG_ROOT` is not `/`, because they currently
run on the host and are not chrooted into the target root. Hook failures warn
but do not roll back the operation.
