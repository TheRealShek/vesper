PREFIX ?= /usr/local
BINDIR = $(PREFIX)/bin
DATADIR = $(PREFIX)/share
APPID = io.github.TheRealShek.vesper

.PHONY: install uninstall

install:
	install -d $(DESTDIR)$(BINDIR)
	install -m 755 target/release/vesper $(DESTDIR)$(BINDIR)/vesper
	install -d $(DESTDIR)$(DATADIR)/applications
	install -m 644 $(APPID).desktop $(DESTDIR)$(DATADIR)/applications/$(APPID).desktop
	install -d $(DESTDIR)$(DATADIR)/icons/hicolor/512x512/apps
	install -m 644 assets/logo.png $(DESTDIR)$(DATADIR)/icons/hicolor/512x512/apps/$(APPID).png
	@echo "Updating system caches..."
	-gtk-update-icon-cache -f -t $(DESTDIR)$(DATADIR)/icons/hicolor
	-update-desktop-database $(DESTDIR)$(DATADIR)/applications

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/vesper
	rm -f $(DESTDIR)$(DATADIR)/applications/$(APPID).desktop
	rm -f $(DESTDIR)$(DATADIR)/icons/hicolor/512x512/apps/$(APPID).png
	@echo "Updating system caches..."
	-gtk-update-icon-cache -f -t $(DESTDIR)$(DATADIR)/icons/hicolor
	-update-desktop-database $(DESTDIR)$(DATADIR)/applications
