# We cannot use $(shell pwd), which will return unix path format on Windows,
# making it hard to use.
cur_dir = $(dir $(abspath $(lastword $(MAKEFILE_LIST))))
contract_workspace = $(cur_dir)/..

record-codehash: clean build
	./target/release/code_hash

build: debug-build release-build

debug-build:
	cd $(contract_workspace) && capsule build

release-build:
	cd $(contract_workspace) && capsule build --release

clean:
	cd $(contract_workspace) && capsule clean

build-codehash-tool:
	cargo build --release

.PHONY: debug-build release-build build clean record-codehash build-codehash-tool

##################################
# Script: Find commit by code_hash

# contract cluster v1 code_hash: 0x598d793defef36e2eeba54a9b45130e4ca92822e1d193671f490950c3b856080
# export CODE_HASH="0x598d793defef36e2eeba54a9b45130e4ca92822e1d193671f490950c3b856080"

# while true; do
#   make record-codehash

#   if grep -q "$CODE_HASH" code_hash.md; then 
#     echo "found!"
#     break;
#   fi
  
#   git checkout HEAD^
#   sleep 1
# done
