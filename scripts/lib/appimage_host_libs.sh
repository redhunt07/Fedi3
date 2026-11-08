#!/usr/bin/env bash
# Libraries that must agree with the host desktop, its modules and configuration.
# GLib is supplied by the host (including libsecret integration), so its base
# dependencies must not be shadowed by older libraries from the build machine.
# Fontconfig also reads the host's configuration, not the build machine's files.
# Background: https://github.com/AppImageCommunity/pkg2appimage/blob/master/excludelist
appimage_uses_host_library() {
  case "$1" in
    ld-linux*|libc.so.*|libm.so.*|libmvec.so.*|libpthread.so.*|librt.so.*|libdl.so.*|libresolv.so.*|libutil.so.*|libanl.so.*|libnss_*.so.*|libstdc++.so.*|libgcc_s.so.*)
      return 0 ;;
    libEGL.so.*|libGL.so.*|libGLX.so.*|libGLdispatch.so.*|libOpenGL.so.*|libgbm.so.*|libdrm.so.*|libwayland-*.so.*|libxshmfence.so.*|libX11.so.*|libX11-xcb.so.*|libxcb.so.*)
      return 0 ;;
    libglib-2.0.so.*|libgobject-2.0.so.*|libgio-2.0.so.*|libgmodule-2.0.so.*|libgthread-2.0.so.*|libmount.so.*|libblkid.so.*|libsystemd.so.*|libudev.so.*|libpcre2-8.so.*|libffi.so.*)
      return 0 ;;
    libfontconfig.so.*|libfreetype.so.*|libharfbuzz.so.*|libexpat.so.*|libz.so.*)
      return 0 ;;
    # GStreamer plugins live on the host in /usr/lib/gstreamer-1.0 and must be
    # loaded by the same libgstreamer they were built against. Bundling the
    # core libraries makes audioplayers fail to create its playbin elements.
    libgst*.so.*)
      return 0 ;;
  esac
  return 1
}
