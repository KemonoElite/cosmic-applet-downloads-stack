name := 'cosmic-applet-downloads-stack'
appid := 'io.github.yourname.DownloadsStack'

rootdir := ''
prefix := '/usr'
base-dir := absolute_path(clean(rootdir / prefix))

bin-dst := base-dir / 'bin' / name
desktop-dst := base-dir / 'share' / 'applications' / appid + '.desktop'
icon-dst := base-dir / 'share' / 'icons' / 'hicolor' / 'scalable' / 'apps' / appid + '.svg'

default: build-release

build-release *args:
    cargo build --release {{args}}

run *args:
    cargo run --release {{args}}

install:
    install -Dm0755 target/release/{{name}} {{bin-dst}}
    install -Dm0644 resources/{{appid}}.desktop {{desktop-dst}}
    install -Dm0644 resources/{{appid}}.svg {{icon-dst}}

uninstall:
    rm -f {{bin-dst}} {{desktop-dst}} {{icon-dst}}

# Build and install the Flatpak for your user (to test before publishing)
flatpak:
    flatpak-builder --user --install --force-clean --install-deps-from=flathub build-dir {{appid}}.json
