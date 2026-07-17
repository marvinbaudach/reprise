#!/usr/bin/env python3
# Minimal stock-GTK repro for: GtkColumnView column-header drag-reorder never
# triggers, despite reorderable=TRUE (observed on GTK 4.22.4; the relevant
# code is unchanged on gtk main as of 2026-07).
#
# Mechanism (from reading gtk/gtkcolumnviewtitle.c + gtk/gtkcolumnview.c):
# GtkColumnViewTitle's own GtkGestureClick claims the event sequence
# unconditionally on *press* (click_pressed_cb). GtkColumnView's reorder
# machinery lives in a capture-phase GtkGestureDrag one level up, but it only
# claims the sequence lazily in header_drag_update, once the pointer crosses
# the click-vs-drag threshold. The title's unconditional claim-on-press wins
# that race every time and cancels the drag gesture before its threshold
# check can ever pass — so a header drag never reorders; the release lands in
# the title's click gesture as a plain sort click instead. (Column *resize*
# is unaffected: its branch claims immediately on press, in capture, before
# the title sees anything.)
#
# Repro (headless; a real desktop session works the same):
#   Xvfb :99 -screen 0 1200x500x24 & DISPLAY=:99 openbox &
#   DISPLAY=:99 GDK_BACKEND=x11 python3 gtk-columnview-header-drag.py &
#   # smooth-drag the "Alpha" header ~250px to the right with xdotool:
#   #   press at the header, ~20 small mousemove steps, release
#   # observed:  BEFORE == AFTER (['Alpha','Beta','Gamma']), no
#   #            items-changed on view.get_columns() at any point
#   # expected:  the drag reorders the columns and items-changed fires
import gi
gi.require_version("Gtk", "4.0")
from gi.repository import Gtk, GLib, Gio, GObject


class Row(GObject.Object):
    def __init__(self, a, b, c):
        super().__init__()
        self.a, self.b, self.c = a, b, c


def make_col(title, attr):
    f = Gtk.SignalListItemFactory()
    f.connect("setup", lambda _f, li: li.set_child(Gtk.Label()))
    f.connect("bind", lambda _f, li: li.get_child().set_label(getattr(li.get_item(), attr)))
    col = Gtk.ColumnViewColumn(title=title, factory=f)
    col.set_resizable(True)
    return col


def order(view):
    cols = view.get_columns()
    return [cols.get_item(i).get_title() for i in range(cols.get_n_items())]


def main():
    app = Gtk.Application(application_id="org.example.ColDrag")

    def activate(app):
        win = Gtk.ApplicationWindow(application=app, default_width=900, default_height=400)
        store = Gio.ListStore()
        for i in range(20):
            store.append(Row(f"a{i}", f"b{i}", f"c{i}"))
        view = Gtk.ColumnView(model=Gtk.NoSelection(model=store))
        view.set_reorderable(True)
        for t, a in (("Alpha", "a"), ("Beta", "b"), ("Gamma", "c")):
            view.append_column(make_col(t, a))
        sw = Gtk.ScrolledWindow()
        sw.set_child(view)
        win.set_child(sw)
        win.present()
        view.get_columns().connect(
            "items-changed", lambda *_: print("COLUMNS-CHANGED:", order(view), flush=True)
        )
        GLib.timeout_add(500, lambda: (print("BEFORE:", order(view), flush=True), False)[1])
        GLib.timeout_add(6000, lambda: (print("AFTER:", order(view), flush=True), app.quit(), False)[2])

    app.connect("activate", activate)
    app.run(None)


main()
