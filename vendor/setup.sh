#!/usr/bin/env bash

HGVERSION=6.7.2

curl -fsSL https://foss.heptapod.net/mercurial/mercurial-devel/-/archive/${HGVERSION}/mercurial-devel-${HGVERSION}.tar.gz | tar xzf -
mv mercurial-devel-${HGVERSION} mercurial-devel
