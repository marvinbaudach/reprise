app_id = 'org.reprise.Reprise'

install_data(
  app_id + '.desktop',
  install_dir: get_option('datadir') / 'applications',
)

install_data(
  app_id + '.metainfo.xml',
  install_dir: get_option('datadir') / 'metainfo',
)

install_data(
  'icons/hicolor/scalable/apps/' + app_id + '.svg',
  install_dir: get_option('datadir') / 'icons/hicolor/scalable/apps',
)

install_data(
  'icons/hicolor/symbolic/apps/' + app_id + '-symbolic.svg',
  install_dir: get_option('datadir') / 'icons/hicolor/symbolic/apps',
)


# The activatable runtime service (plan section 9.4). Binary, bus-activation
# file and systemd user unit are installed together and by the same build:
# a runtime whose metadata is missing is not "degraded", it simply never
# starts on a user's machine while working perfectly on a developer's.
# `scripts/check-runtime-service-install.sh` keeps the three in agreement.
runtime_service_config = configuration_data()
runtime_service_config.set('bindir', prefix / get_option('bindir'))

configure_file(
  input: 'org.reprise.Reprise1.service.in',
  output: 'org.reprise.Reprise1.service',
  configuration: runtime_service_config,
  install: true,
  install_dir: get_option('datadir') / 'dbus-1/services',
)

# `lib/systemd/user` is where system packages ship user units, and it is on
# systemd's search path for a `/usr` prefix — but `systemd-analyze --user
# unit-paths` does NOT include `<prefix>/lib/systemd/user` for an arbitrary
# prefix, only the hardcoded /usr and /usr/local variants. README.md's
# documented developer/user install is `--prefix="$HOME/.local"`, and under
# that prefix `lib/systemd/user` is invisible to systemd while
# `share/systemd/user` is on the search path (as is /usr/share/systemd/user
# for a /usr prefix). `datadir` (= 'share') is therefore the one directory
# under the unit's `<prefix>/<dir>/systemd/user` shape that systemd actually
# searches for both prefixes this project supports.
# `scripts/check-runtime-service-install.sh` verifies this for both prefixes.
configure_file(
  input: 'reprise-runtime.service.in',
  output: 'reprise-runtime.service',
  configuration: runtime_service_config,
  install: true,
  install_dir: prefix / get_option('datadir') / 'systemd/user',
)
