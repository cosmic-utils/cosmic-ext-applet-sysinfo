default: build

build:
	cargo build --release

export NAME := 'cosmic-ext-applet-sysinfo'
export APPID := 'io.github.rwxroot.' + NAME

cargo-target-dir := env('CARGO_TARGET_DIR', 'target')
bin-src := cargo-target-dir / 'release' / NAME

rootdir := ''
prefix := '/usr'

base-dir := absolute_path(clean(rootdir / prefix))
share-dst := base-dir / 'share'

bin-dst := base-dir / 'bin' / NAME
desktop-dst := share-dst / 'applications' / APPID + '.desktop'
icon-dst := share-dst / 'icons/hicolor/scalable/apps' / APPID + '-symbolic.svg'

install:
	install -Dm0755 {{ bin-src }} {{ bin-dst }}
	install -Dm0644 extra/applet_icon.svg {{ icon-dst }}
	install -Dm0644 extra/applet_sysinfo.desktop {{ desktop-dst }}

uninstall:
	rm {{ bin-dst }}
	rm {{ icon-dst }}
	rm {{ desktop-dst }}
