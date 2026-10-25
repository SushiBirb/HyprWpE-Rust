import sys
import os

# Force LD_PRELOAD=/usr/lib/libgtk4-layer-shell.so before GTK4 initializes libwayland-client
# to guarantee layer-shell surface creation instead of regular window creation
LAYER_SHELL_LIB = "/usr/lib/libgtk4-layer-shell.so"
if os.path.isfile(LAYER_SHELL_LIB) and LAYER_SHELL_LIB not in os.environ.get("LD_PRELOAD", ""):
    os.environ["LD_PRELOAD"] = LAYER_SHELL_LIB + ((":" + os.environ["LD_PRELOAD"]) if os.environ.get("LD_PRELOAD") else "")
    os.execv(sys.executable, [sys.executable] + sys.argv)

import gi
gi.require_version('Gtk', '4.0')
gi.require_version('WebKit', '6.0')
gi.require_version('Gtk4LayerShell', '1.0')
from gi.repository import Gtk, WebKit, Gtk4LayerShell, Gdk, Gio, GLib
import http.server
import socketserver
import threading
import functools
import yaml

CONFIG_DIR = os.path.expanduser("~/.config/HyprWpE")
PROPERTIES_FILE = os.path.join(CONFIG_DIR, "properties.yaml")

def load_properties():
    if os.path.exists(PROPERTIES_FILE):
        try:
            with open(PROPERTIES_FILE, 'r') as f:
                return yaml.safe_load(f) or {}
        except Exception:
            pass
    return {}

class WebWallpaperWindow(Gtk.ApplicationWindow):
    def __init__(self, *args, monitor_name=None, **kwargs):
        super().__init__(*args, **kwargs)
        
        self.set_decorated(False)
        self.webview = WebKit.WebView()
        settings = self.webview.get_settings()
        
        for prop, val in [
            ("media-playback-allows-inline", True),
            ("allow-file-access-from-file-urls", True),
            ("allow-universal-access-from-file-urls", True),
            ("enable-webgl", True),
            ("enable-developer-extras", True)
        ]:
            try:
                settings.set_property(prop, val)
            except Exception:
                pass

        self.webview.connect("web-process-terminated", self.on_web_process_terminated)
        self.set_child(self.webview)
        
        Gtk4LayerShell.init_for_window(self)
        Gtk4LayerShell.set_namespace(self, "wallpaper")
        Gtk4LayerShell.set_layer(self, Gtk4LayerShell.Layer.BACKGROUND)
        Gtk4LayerShell.set_exclusive_zone(self, -1)
        Gtk4LayerShell.set_keyboard_mode(self, Gtk4LayerShell.KeyboardMode.NONE)

        if monitor_name:
            display = Gdk.Display.get_default()
            monitors = display.get_monitors()
            for monitor in monitors:
                if monitor.get_connector() == monitor_name:
                    Gtk4LayerShell.set_monitor(self, monitor)
                    break
        
        props = load_properties()
        margins = props.get("panel_margins", {})
        Gtk4LayerShell.set_margin(self, Gtk4LayerShell.Edge.TOP, margins.get("top", 0))
        Gtk4LayerShell.set_margin(self, Gtk4LayerShell.Edge.BOTTOM, margins.get("bottom", 0))
        Gtk4LayerShell.set_margin(self, Gtk4LayerShell.Edge.LEFT, margins.get("left", 0))
        Gtk4LayerShell.set_margin(self, Gtk4LayerShell.Edge.RIGHT, margins.get("right", 0))

        Gtk4LayerShell.set_anchor(self, Gtk4LayerShell.Edge.TOP, True)
        Gtk4LayerShell.set_anchor(self, Gtk4LayerShell.Edge.BOTTOM, True)
        Gtk4LayerShell.set_anchor(self, Gtk4LayerShell.Edge.LEFT, True)
        Gtk4LayerShell.set_anchor(self, Gtk4LayerShell.Edge.RIGHT, True)

    def on_web_process_terminated(self, webview, reason):
        print(f"[HyprWpE] Web process terminated (reason={reason}). Reloading...", file=sys.stderr)
        GLib.timeout_add(1000, lambda: self.webview.reload() or False)

    def load_uri(self, uri):
        self.webview.load_uri(uri)

class WebWallpaperApp(Gtk.Application):
    def __init__(self, uri, monitor_name, *args, **kwargs):
        super().__init__(
            *args, 
            application_id="dev.gemini.hyprpaperwe.viewer", 
            flags=Gio.ApplicationFlags.NON_UNIQUE,
            **kwargs
        )
        self.uri = uri
        self.monitor_name = monitor_name
        self.win = None

    def do_activate(self):
        if not self.win:
            self.win = WebWallpaperWindow(application=self, monitor_name=self.monitor_name)
        self.win.load_uri(self.uri)
        self.win.present()

class CORSHTTPRequestHandler(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header('Access-Control-Allow-Origin', '*')
        self.send_header('Cache-Control', 'no-store, no-cache, must-revalidate')
        super().end_headers()

    def guess_type(self, path):
        if path.endswith('.json'): return 'application/json'
        if path.endswith('.vert') or path.endswith('.frag') or path.endswith('.glsl'): return 'text/plain'
        return super().guess_type(path)


def start_local_http_server(directory):
    handler = functools.partial(CORSHTTPRequestHandler, directory=directory)
    httpd = socketserver.TCPServer(('127.0.0.1', 0), handler)
    port = httpd.socket.getsockname()[1]
    t = threading.Thread(target=httpd.serve_forever, daemon=True)
    t.start()
    return port

if __name__ == "__main__":
    if not (2 <= len(sys.argv) <= 3):
        print(f"Usage: {sys.argv[0]} <html_file_path_or_url> [monitor_name]", file=sys.stderr)
        sys.exit(1)

    target_input = sys.argv[1]
    monitor_name = sys.argv[2] if len(sys.argv) == 3 else None

    if target_input.startswith("http://") or target_input.startswith("https://"):
        uri = target_input
    else:
        html_path = os.path.abspath(target_input)
        if os.path.isdir(html_path):
            html_path = os.path.join(html_path, "index.html")

        if not os.path.isfile(html_path):
            print(f"[HyprWpE] Warning: {html_path} not found. Preparing wallpaper directory...", file=sys.stderr)

        root_dir = os.path.dirname(html_path)
        filename = os.path.basename(html_path)
        port = start_local_http_server(root_dir)
        
        props = load_properties()
        wp_id = os.path.basename(root_dir)
        wp_speed = props.get(wp_id, {}).get("speed", 0.8)

        uri = f"http://127.0.0.1:{port}/{filename}?speed={wp_speed}"

    app = WebWallpaperApp(uri, monitor_name)
    app.run([sys.argv[0]])
