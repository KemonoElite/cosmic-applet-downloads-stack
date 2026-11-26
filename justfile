name := 'cosmic-applet-downloads-stack'
appid := 'io.github.KemonoElite.CosmicAppletDownloadsStack'

rootdir := ''
prefix := '/usr'
debug := '0'
target := if debug == '1' { 'debug' } else { 'release' }
targetdir := env('CARGO_TARGET_DIR', 'target')
sharedir := rootdir + prefix + '/share'
iconsdir := sharedir + '/icons/hicolor'
bindir := rootdir + prefix + '/bin'

bin-dst := bindir / name
desktop-src := targetdir / 'xdgen' / appid + '.desktop'
desktop-dst := sharedir / 'applications' / appid + '.desktop'
metainfo-src := 'data' / appid + '.metainfo.xml'
metainfo-dst := sharedir / 'metainfo' / appid + '.metainfo.xml'
icon-src := 'data' / 'icons' / 'scalable' / 'apps' / appid + '-symbolic.svg'
icon-dst := iconsdir / 'scalable' / 'apps' / appid + '-symbolic.svg'

default: build-release

# Compiles with debug profile
build-debug *args:
    cargo build {{ args }}

# Compiles with release profile
build-release *args: (build-debug '--release' args)

# Runs clippy the same way CI would
check *args:
    cargo clippy --all-features {{ args }} -- -W clippy::pedantic

# Runs the applet from the build directory (open it from a COSMIC session)
run *args:
    env RUST_LOG=cosmic_applet_downloads_stack=debug RUST_BACKTRACE=full cargo run --release {{ args }}

# Installs files into the system
install:
    install -Dm0755 {{ targetdir }}/{{ target }}/{{ name }} {{ bin-dst }}
    install -Dm0644 {{ desktop-src }} {{ desktop-dst }}
    install -Dm0644 {{ metainfo-src }} {{ metainfo-dst }}
    install -Dm0644 {{ icon-src }} {{ icon-dst }}

# Uninstalls installed files
uninstall:
    rm -f {{ bin-dst }} {{ desktop-dst }} {{ metainfo-dst }} {{ icon-dst }}
