ifndef PREFIX
	PREFIX = /usr/local/bin
endif

install: rush.rs
	rustc -C opt-level=1 rush.rs
	strip ./rush >/dev/null 2>&1 # (optional) decrease executable size
	sudo cp ./rush $(PREFIX)/
	sudo mkdir -p /usr/share/rush
	sudo cp ./rush.bash /usr/share/rush/
	sed -i 's_^#\+source /usr/share/rush/rush.bash_source /usr/share/rush/rush.bash_' ~/.bashrc || echo "source /usr/share/rush/rush.bash >/dev/null 2>&1" >> ~/.bashrc


uninstall:
	sudo rm -f $(PREFIX)/rush >/dev/null 2>&1
	sudo rm -rf /usr/share/rush >/dev/null 2>&1
	sed -i 's_source /usr/share/rush/rush.bash_#source /usr/share/rush/rush.bash_' ~/.bashrc
