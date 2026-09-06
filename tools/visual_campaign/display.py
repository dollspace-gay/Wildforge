"""Own an isolated X11 display and request normal native window closure."""

import contextlib
import ctypes
import ctypes.util
from pathlib import Path
import select
import subprocess


@contextlib.contextmanager
def isolated_display(work: Path):
    # Xvfb owns the display/input surface. The game must still report its real
    # discrete Vulkan adapter; this does not provide a rendering fallback.
    with (work / "motion-display.log").open("a") as log:
        server = subprocess.Popen(
            ["Xvfb", "-displayfd", "1", "-screen", "0", "1920x1080x24", "-nolisten", "tcp", "-noreset"],
            stdout=subprocess.PIPE, stderr=log, text=True,
        )
        try:
            if not select.select([server.stdout], [], [], 30)[0]:
                raise TimeoutError("isolated X11 display did not start")
            number = server.stdout.readline().strip()
            if not number.isdigit() or server.poll() is not None:
                raise RuntimeError("isolated X11 display failed to start")
            yield ":" + number
        finally:
            server.stdout.close()
            if server.poll() is None:
                server.terminate()
                try:
                    server.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.wait()


class ClientData(ctypes.Union):
    _fields_ = [("longs", ctypes.c_long * 5)]


class ClientMessage(ctypes.Structure):
    _fields_ = [("type", ctypes.c_int), ("serial", ctypes.c_ulong), ("send_event", ctypes.c_int),
                ("display", ctypes.c_void_p), ("window", ctypes.c_ulong), ("message_type", ctypes.c_ulong),
                ("format", ctypes.c_int), ("data", ClientData)]


class Event(ctypes.Union):
    _fields_ = [("client", ClientMessage), ("padding", ctypes.c_long * 24)]


def request_close(display: str, window: str) -> None:
    """Send ICCCM WM_DELETE_WINDOW directly, including on displays without a WM.

    https://www.x.org/releases/X11R7.7/doc/xorg-docs/icccm/icccm.pdf
    Unlike destroying a window, this lets the game save and join its workers.
    """
    library = ctypes.util.find_library("X11")
    if not library:
        raise RuntimeError("libX11 is required for native motion recording")
    x11 = ctypes.CDLL(library)
    x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x11.XOpenDisplay.restype = ctypes.c_void_p
    x11.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
    x11.XInternAtom.restype = ctypes.c_ulong
    x11.XSendEvent.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_long, ctypes.POINTER(Event)]
    x11.XFlush.argtypes = [ctypes.c_void_p]
    x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
    connection = x11.XOpenDisplay(display.encode())
    if not connection:
        raise RuntimeError("could not connect to the owned motion display")
    try:
        event = Event()
        event.client.type = 33  # ClientMessage
        event.client.display = connection
        event.client.window = int(window)
        event.client.message_type = x11.XInternAtom(connection, b"WM_PROTOCOLS", False)
        event.client.format = 32
        event.client.data.longs[0] = x11.XInternAtom(connection, b"WM_DELETE_WINDOW", False)
        if not x11.XSendEvent(connection, int(window), False, 0, ctypes.byref(event)):
            raise RuntimeError("normal window close request failed")
        x11.XFlush(connection)
    finally:
        x11.XCloseDisplay(connection)
