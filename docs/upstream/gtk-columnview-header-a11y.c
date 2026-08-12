/*
 * Minimal reproducer for a GTK4 accessibility bug:
 *
 * A GtkColumnViewColumn with a sorter attached via
 * gtk_column_view_column_set_sorter() renders a clickable column header
 * (internally a GtkColumnViewTitle, CSS name "button"), but that header
 * exposes NO AT-SPI Action interface / no actions at all. Its AT-SPI role
 * is "filler", not "push button", so assistive technology has no way to
 * trigger the sort that a mouse click can trigger.
 *
 * For comparison, this program also creates:
 *   - a second column with no sorter attached, to show the header of a
 *     non-sortable column looks the same on the bus (no action either way)
 *   - a plain GtkButton as a control, to show what a widget that DOES
 *     expose a working AT-SPI action looks like in the same tree.
 *
 * Build:
 *   cc columnview-header-a11y.c -o columnview-header-a11y \
 *       $(pkg-config --cflags --libs gtk4)
 */

#include <gtk/gtk.h>

static void
setup_label_cb (GtkListItemFactory *factory, GtkListItem *item, gpointer user_data)
{
  (void) factory;
  (void) user_data;
  GtkWidget *label = gtk_label_new ("");
  gtk_label_set_xalign (GTK_LABEL (label), 0.0f);
  gtk_list_item_set_child (item, label);
}

static void
bind_label_cb (GtkListItemFactory *factory, GtkListItem *item, gpointer user_data)
{
  (void) factory;
  (void) user_data;
  GtkWidget *label = gtk_list_item_get_child (item);
  GtkStringObject *entry = gtk_list_item_get_item (item);
  gtk_label_set_text (GTK_LABEL (label), gtk_string_object_get_string (entry));
}

static GtkListItemFactory *
make_text_factory (void)
{
  GtkListItemFactory *factory = gtk_signal_list_item_factory_new ();
  g_signal_connect (factory, "setup", G_CALLBACK (setup_label_cb), NULL);
  g_signal_connect (factory, "bind", G_CALLBACK (bind_label_cb), NULL);
  return factory;
}

static void
activate (GtkApplication *app, gpointer user_data)
{
  (void) user_data;

  GtkWidget *window = gtk_application_window_new (app);
  gtk_window_set_title (GTK_WINDOW (window), "ColumnView Header A11y Repro");
  gtk_window_set_default_size (GTK_WINDOW (window), 480, 320);

  GtkWidget *box = gtk_box_new (GTK_ORIENTATION_VERTICAL, 6);
  gtk_widget_set_margin_top (box, 6);
  gtk_widget_set_margin_bottom (box, 6);
  gtk_widget_set_margin_start (box, 6);
  gtk_widget_set_margin_end (box, 6);
  gtk_window_set_child (GTK_WINDOW (window), box);

  /* Control widget: a normal button, expected to expose a working
   * AT-SPI "click" action. Used to contrast against the column headers. */
  GtkWidget *control_button = gtk_button_new_with_label ("Control Button");
  gtk_box_append (GTK_BOX (box), control_button);

  /* Backing model for the column view: a small GtkStringList. */
  const char *const items[] = { "Banana", "Apple", "Cherry", NULL };
  GtkStringList *model = gtk_string_list_new (items);

  GtkSingleSelection *selection =
    gtk_single_selection_new (G_LIST_MODEL (model));

  GtkWidget *column_view =
    gtk_column_view_new (GTK_SELECTION_MODEL (selection));
  gtk_widget_set_vexpand (column_view, TRUE);

  /* Column 1: header WITH a sorter attached. This is the header that,
   * per the bug being reported, does not expose an AT-SPI action. */
  GtkColumnViewColumn *sortable_column =
    gtk_column_view_column_new ("Sortable", make_text_factory ());
  gtk_column_view_column_set_expand (sortable_column, TRUE);
  gtk_column_view_column_set_sorter (
    sortable_column,
    GTK_SORTER (gtk_string_sorter_new (
      gtk_property_expression_new (GTK_TYPE_STRING_OBJECT, NULL, "string"))));
  gtk_column_view_append_column (GTK_COLUMN_VIEW (column_view),
                                  sortable_column);
  g_object_unref (sortable_column);

  /* Column 2: header WITHOUT a sorter attached, for comparison. */
  GtkColumnViewColumn *plain_column =
    gtk_column_view_column_new ("Plain", make_text_factory ());
  gtk_column_view_column_set_expand (plain_column, TRUE);
  gtk_column_view_append_column (GTK_COLUMN_VIEW (column_view), plain_column);
  g_object_unref (plain_column);

  gtk_box_append (GTK_BOX (box), column_view);

  gtk_window_present (GTK_WINDOW (window));
}

int
main (int argc, char *argv[])
{
  /* Fixed prgname/application name so the AT-SPI dump can find this
   * process by name on the accessibility bus. */
  g_set_prgname ("ColumnViewHeaderA11y");
  g_set_application_name ("ColumnViewHeaderA11y");

  GtkApplication *app = gtk_application_new (
    "org.gtk.bugreport.ColumnViewHeaderA11y", G_APPLICATION_DEFAULT_FLAGS);
  g_signal_connect (app, "activate", G_CALLBACK (activate), NULL);

  int status = g_application_run (G_APPLICATION (app), argc, argv);
  g_object_unref (app);
  return status;
}
