---
slug: queue-centering-ignores-section-headers
worktree:
branch:
phase: todo
codex_session:
created: 2026-08-19
---
# TODO: Die Zentrierung rechnet ohne Sektionsköpfe

**Nur ein Befund, kein Plan.** Aus dem Grill zu
`one-centering-path-for-jump-and-clear` als eigener Fehler abgetrennt (siehe
dort „Nicht in diesem Plan"), damit er den dortigen Umbau nicht mitzieht.

## Der Befund

Beide Zentrierpfade rechnen reine Zeilenmathematik und lassen die Höhe der
Sektionsköpfe aus dem Zielwert heraus:

- `reload_restore::centered_track_scroll_target` nimmt `row_height` und
  `viewport_height` und reicht sie an
  `scroll_center::centered_scroll_value_with_height` durch;
- `centered_scroll_value_with_height` rechnet
  `content_height = n_rows * row_height` und
  `target = (position + 0.5) * row_height - page/2`.

Der Ankerpfad macht es anders und richtig: `reload_anchor_scroll` geht über
`list_geometry_layout::ListLayout`, und `headers_above_in` zählt die Kopfzeilen
oberhalb der Zielzeile mit.

Bemerkenswert ist, dass `centered_scroll_restore::apply` die Sektionszahl
sogar **kennt** — es liest `shared.queue_sections.borrow().len()` und gibt sie
an `content_height` weiter, um zu entscheiden, ob der Inhalt in den Viewport
passt. Für den Zielwert selbst wird sie dann nicht benutzt.

## Wo es sich auswirkt

Nur dort, wo die Ansicht sektioniert ist — das ist die Warteschlange
(`QueueViewModel`). In einer flachen Bibliotheksansicht ist `n_sections == 0`
und die Rechnung stimmt. In der Warteschlange landet die zentrierte Zeile um
die Summe der Kopfhöhen oberhalb daneben, also je weiter unten der Titel
steht, desto weiter.

**Nicht gemessen.** Der Befund ist am Code belegt, nicht an einem Screenshot;
wie groß der Versatz in der Praxis ist, gehört zum ersten Schritt eines echten
Plans.

## Berührung durch den Zentrierpfad-Umbau

Der Plan `one-centering-path-for-jump-and-clear` hat genau diese Stelle
angefasst — und dabei **eine Hälfte des Fehlers mitgenommen**, weil der Umbau
sie nicht umgehen konnte.

**Der Wiederherstellpfad rechnet die Kopfzeilen jetzt mit.** Er landet nicht
mehr auf dem arithmetischen Mittelwert, sondern auf der Zeilenkante, die ihm
am nächsten liegt — das ist der einzige Wert, den GTKs eigener Anker
reproduziert. Diese Kante muss aus derselben Geometrie kommen, die GTK
benutzt, sonst schreibt der Allokationsdurchlauf sie wieder um. Also geht
`centered_scroll_restore::centered_anchor` über `ListLayout::row_top`, und
damit zählen die Kopfzeilen mit. Der Unit-Test
`section_headers_move_the_edge_the_anchor_names` hält das fest.

**Der Sprungpfad rechnet sie weiterhin nicht mit.** `RevealMotion::Glide` —
also der Titelwechsel — geht nach wie vor über
`scroll_center::centered_scroll_target` →
`centered_scroll_value_with_height`, und dort ist die Rechnung reine
Zeilenmathematik geblieben. **Das ist ab jetzt die Adresse dieses Fehlers.**

Der Grund für die Halbierung ist kein Versäumnis, sondern eine andere Lage:
der Sprungpfad läuft auf einer stehenden Liste, ohne Modelltausch und ohne
GTKs Anker-Wiederherstellung, also zwingt ihn nichts auf eine Zeilenkante und
er darf den exakten Mittelwert schreiben. Er müsste dafür nur die Kopfhöhen
addieren.

**Weiterhin nicht gemessen.** Wie groß der Versatz in einer echten
Warteschlange ist, gehört immer noch zum ersten Schritt eines echten Plans.
