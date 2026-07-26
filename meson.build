project(
  'reprise',
  'rust',
  version: '0.1.1',
  license: 'GPL-3.0-or-later',
  meson_version: '>= 1.3.0',
)

gnome = import('gnome')

prefix = get_option('prefix')
localedir = prefix / get_option('localedir')
cargo_build = find_program('build-aux/meson-cargo-build.sh')

custom_target(
  'reprise',
  output: 'reprise',
  command: [
    cargo_build,
    meson.project_source_root(),
    meson.project_build_root(),
    '@OUTPUT@',
    localedir,
    get_option('stem_backend').to_string(),
    prefix,
  ],
  env: {'MESON_BUILD_PROFILE': get_option('profile')},
  build_by_default: true,
  install: true,
  install_dir: get_option('bindir'),
)

subdir('data')
subdir('po')

gnome.post_install(
  gtk_update_icon_cache: true,
  update_desktop_database: true,
)
