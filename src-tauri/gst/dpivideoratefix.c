/*
 * GStreamer tracer: nastaví `skip-to-first` každému prvku videorate.
 *
 * Obchází chybu WebKitGTK s GStreamerem >= 1.28: capture pipeline kamery má
 * nulový base time, takže snímky nesou absolutní časy (uptime systému)
 * a videorate „vyplňuje“ mezeru od nuly duplikáty prvního snímku — náhled
 * kamery zamrzne na prvním snímku (na hodiny dle uptime).
 * https://github.com/WebKit/WebKit/pull/74373
 *
 * Načítá se přes GST_PLUGIN_PATH + GST_TRACERS (viz src-tauri/src/main.rs),
 * proměnné dědí i procesy WebKitu.
 */
#include <gst/gst.h>

#define PACKAGE "dpi"

typedef struct { GstTracer parent; } DpiVideorateFix;
typedef struct { GstTracerClass parent_class; } DpiVideorateFixClass;

G_DEFINE_TYPE (DpiVideorateFix, dpi_videorate_fix, GST_TYPE_TRACER)

static void
on_element_new (GObject * self, GstClockTime ts, GstElement * element)
{
  GstElementFactory *f = gst_element_get_factory (element);
  if (f && g_strcmp0 (GST_OBJECT_NAME (f), "videorate") == 0
      && g_object_class_find_property (G_OBJECT_GET_CLASS (element), "skip-to-first"))
    g_object_set (element, "skip-to-first", TRUE, NULL);
}

static void
dpi_videorate_fix_class_init (DpiVideorateFixClass * klass)
{
}

static void
dpi_videorate_fix_init (DpiVideorateFix * self)
{
  gst_tracing_register_hook (GST_TRACER (self), "element-new",
      G_CALLBACK (on_element_new));
}

static gboolean
plugin_init (GstPlugin * plugin)
{
  return gst_tracer_register (plugin, "dpivideoratefix",
      dpi_videorate_fix_get_type ());
}

GST_PLUGIN_DEFINE (GST_VERSION_MAJOR, GST_VERSION_MINOR, dpivideoratefix,
    "videorate skip-to-first workaround (WebKitGTK camera freeze)", plugin_init,
    "1.0", "GPL", "dpi", "https://github.com/Sherlock3721/DPI")
