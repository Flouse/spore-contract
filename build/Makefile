# We cannot use $(shell pwd), which will return unix path format on Windows,
# making it hard to use.
cur_dir = $(dir $(abspath $(lastword $(MAKEFILE_LIST))))
contract_workspace = $(cur_dir)/..

build: debug-build release-build

debug-build:
	cd $(contract_workspace) && capsule build

release-build:
	cd $(contract_workspace) && capsule build --release

clean:
	cd $(contract_workspace) && capsule clean

.PHONY: debug-build release-build build clean
