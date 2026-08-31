app_id = 'io.github.marvinbaudach.Reprise'

install_data(
  app_id + '.desktop',
  install_dir: get_option('datadir') / 'applications',
)

install_data(
  app_id + '.metainfo.xml',
  install_dir: get_option('datadir') / 'metainfo',
)

# Install the exact scalable drawing and every measured raster stage. The
# smallest stages remain explicit because desktop shells may request them
# without resampling the scalable icon.
install_data(
  'icons/hicolor/scalable/apps/' + app_id + '.svg',
  install_dir: get_option('datadir') / 'icons/hicolor/scalable/apps',
)

foreach size : ['16x16', '22x22', '24x24', '32x32', '48x48', '64x64', '128x128', '256x256', '512x512']
  install_data(
    'icons/hicolor/' + size + '/apps/' + app_id + '.png',
    install_dir: get_option('datadir') / 'icons/hicolor' / size / 'apps',
  )
endforeach

install_data(
  'icons/hicolor/symbolic/apps/' + app_id + '-symbolic.svg',
  install_dir: get_option('datadir') / 'icons/hicolor/symbolic/apps',
)

install_data(
  'icons/hicolor/symbolic/apps/' + app_id + '-first-aid-symbolic.svg',
  install_dir: get_option('datadir') / 'icons/hicolor/symbolic/apps',
)
