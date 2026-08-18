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

Der Plan `one-centering-path-for-jump-and-clear` fasst genau diese Stelle an:
sein Task 3 führt beide Anlässe durch `track_reveal::reveal_position`, und
damit läuft der Wiederherstellpfad künftig über
`scroll_center::centered_scroll_target` statt über
`centered_track_scroll_target`. Der Fehler wandert dadurch nur die Adresse:
`centered_scroll_value_with_height` ignoriert die Kopfzeilen genauso.

Stand 19.08.2026 ist dieser Umbau **nicht** gelandet (die Messung hat ihn
gestoppt, siehe dort). Wer ihn wieder aufnimmt, sollte hier nachtragen, welche
Funktion am Ende die Adresse des Fehlers ist.
