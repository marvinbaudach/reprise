#!/usr/bin/env bash

# Fixed pointer geometry for the mapped 1600x900, five-track fixture window.
# These coordinates address the onboarding banner introduced by 31d8fa062a.
DISCOVERY_BANNER_NOT_NOW_X=1536
DISCOVERY_BANNER_NOT_NOW_Y=71
ROW0_TITLE_CELL_X=355
ROW0_TITLE_CELL_Y=170
ROW1_TITLE_CELL_X=355
ROW1_TITLE_CELL_Y=221
ROW0_RATING_STAR2_X=1536
ROW0_RATING_STAR_Y=175
TITLE_HEADER_X=500
COLUMN_HEADER_Y=136
# The column menu lists Title, Artist, Album, Year, Added at y=260, 329, 398,
# 467, 536 with the toggle switch at x=695. The old 560/208 landed in the empty
# strip above the first row, so nothing was ever toggled.
HEADER_MENU_ARTIST_X=695
HEADER_MENU_ARTIST_Y=329
SIDEBAR_PLAYLIST_X=100
SIDEBAR_PLAYLIST_Y=226
SIDEBAR_PLAYLIST_DELETE_X=100
SIDEBAR_PLAYLIST_DELETE_Y=305
PLAYLIST_DELETE_CONFIRM_X=800
PLAYLIST_DELETE_CONFIRM_Y=475
SIDEBAR_QUEUE_X=80
SIDEBAR_QUEUE_Y=150
# Search is now a compact header toggle instead of a 300px entry. The end
# controls stay right-anchored; record every slot explicitly so future pointer
# recalibration cannot accidentally target the revealed second top bar.
PRIMARY_MENU_FROM_RIGHT=227
SEARCH_TOGGLE_FROM_RIGHT=186
# The current header has no Information toggle, so there is deliberately no
# INFO_TOGGLE_FROM_RIGHT constant to inherit as a stale pointer target.
# `compact_player_layouts.rs` builds one 430x76 mini card. Compact points are
# derived from the live window rect; only the widget-edge offsets stay fixed.
COMPACT_CARD_MAX_WIDTH=430
COMPACT_CARD_MAX_HEIGHT=76
COMPACT_COVER_CENTER_X=34
COMPACT_PLAY_BUTTON_FROM_RIGHT=32
