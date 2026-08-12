# Upstream-Entwürfe für gitlab.gnome.org/GNOME/gtk

**Eingereicht am 12.08.2026: <https://gitlab.gnome.org/GNOME/gtk/-/issues/8354>**
(Konto `Klosterfrau`). **A** ist der Issue-Text, **B** hängt als Kommentar
darunter — `#note_2834740`. C-Beispiel und Dump-Skript sind als aufklappbare
`<details>`-Blöcke im Issue eingebettet, weil `glab` keine Anhänge an ein neues
Issue hängen kann; die Dateien hier sind die Quelle davon.

Das Label `8. Accessibility` liess sich **nicht** setzen: `glab issue update`
meldet „added labels", die API liefert danach weiter `labels: []` — als
Nicht-Mitglied darf man auf GNOMEs Tracker keine Labels vergeben. Das macht die
Triage.

Darunter unverändert die beiden Entwürfe, aus denen der Bericht entstanden ist.

Zwei getrennte Meldungen. **A** ist der eigentliche Bericht (neu, reproduziert,
mit C-Beispiel). **B** ist klein und optional — das zugrundeliegende Verhalten
ist dokumentiert, gemeldet würde nur das stille Auseinanderlaufen.

---

## A — GtkColumnView: sortable column headers expose no AT-SPI action

**Title:** `GtkColumnView: sortable column headers expose no AT-SPI action, so sorting is unreachable for assistive technology`

### Steps to reproduce

1. Build and run the attached minimal example
   (`gtk-columnview-header-a11y.c` in this directory, ~120 lines, GTK only):
   ```
   cc gtk-columnview-header-a11y.c -o columnview-header-a11y $(pkg-config --cflags --libs gtk4)
   ```
   It shows a `GtkColumnView` with two columns — `Sortable`
   (`gtk_column_view_column_set_sorter()` is called on it) and `Plain` (no
   sorter) — plus a plain `GtkButton` as a control.
2. Inspect the accessible tree with Accerciser, or query it over AT-SPI.

### Current behaviour

Both column headers appear with role `filler`. They do carry the `Action`
interface, but `get_n_actions()` returns **0** — no `click`, no sort action.
The sortable and the non-sortable header are indistinguishable on the bus:

```
button   name='Control Button'   actions=[click]      <- plain GtkButton, works
filler   name='Sortable'         actions=[]           <- has a sorter, still no action
filler   name='Plain'            actions=[]
```

No warning or critical is printed.

Since a pointer click on the sortable header does sort the view, this is
functionality that is available to mouse users but not exposed to assistive
technology at all: a screen-reader user cannot sort the view.

### Expected behaviour

A column header that reacts to activation (i.e. its column has a sorter) should
expose that on the AT-SPI bus — an appropriate role (`column header` / `button`)
with an activatable action, so an AT can trigger the same sort the pointer does.

### Notes

`GtkColumnViewTitle` carries the CSS name `button` and looks and behaves like
one, but it is not a `GtkButton`. As far as I can tell, AT-SPI actions in GTK 4
come from the widget type, never from the accessible role: I verified that a
`GtkListBoxRow` constructed with `accessible-role = button` still exposes zero
actions, while a real `GtkButton` exposes `click`. So an application cannot work
around this from the outside — the header widget is internal, and the role
cannot be changed after construction.

Related, but as far as I can see none of these covers this:

- #325 (open, labelled `GtkColumnView`) asks for the *current sort order* of a
  sortable header to be exposed. That is the state; this report is about the
  header offering no way to *change* it — zero actions, so an AT cannot sort at
  all.
- #6583 (open) asks for all widgets to carry an easily-identifiable action name
  that Orca can recognise as a click. The header here has no action to name in
  the first place, so it is arguably a concrete instance of that request; happy
  to have this folded in there if you prefer.
- #6268 (open) covers widgets reporting table/table-cell roles without
  implementing the corresponding AT-SPI interfaces — the missing table
  interfaces, not the missing action on the header.

### Version information

- GTK 4.22.4 (`pkg-config --modversion gtk4`)
- at-spi2-core 2.60.6
- Manjaro Linux, X11 (reproduced on a bare Xvfb server, `GDK_BACKEND=x11`,
  `GSK_RENDERER=cairo`)

---

## B — accessible-role set after construction diverges silently from the bus

**Title:** `Setting accessible-role after construction silently diverges: getter reports the new role, AT-SPI keeps the old one`

### What happens

The docs are explicit that "The accessible role cannot be changed once set", so
the refusal itself is expected. What is surprising is *how* it fails when the
`GtkATContext` has not been realized yet:

- `gtk_accessible_get_accessible_role()` returns the **new** role,
- the AT-SPI node keeps the **default** role (for `GtkListBoxRow`: generic /
  `panel`),
- nothing is printed — no warning, no critical.

Because the effective role stays `generic`, a label set via
`gtk_accessible_update_property(GTK_ACCESSIBLE_PROPERTY_LABEL, …)` is also
dropped (ARIA prohibits author-provided names on `generic`, so that part is
presumably correct) — the widget ends up both role-less and nameless on the bus,
while the application code has every reason to believe it succeeded.

Setting the role as a construction property works correctly.

### Expected

Either refuse consistently and loudly (the realized path already emits a
`g_critical`), or let the getter report the role that is actually in effect. As
it stands, the getter is the natural thing for an application (or its test
suite) to check, and it confirms a state the bus does not have.

### Version information

Same as above: GTK 4.22.4, at-spi2-core 2.60.6, Manjaro, X11.
