# :mate: matepkg :package:

Matepkg is a Linux package manager written in Rust, it is based on how [bananapkg](https://github.com/slackjeff/bananapkg) works, but later on I intend include new features. I'm using it as a learning project, it is not yet complete.

## :star: Requirements (building from source)
* **Rust** >= 1.90.0 <br/>

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
