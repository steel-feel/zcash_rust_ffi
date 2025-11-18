package main

import (
	"unsafe"
)

/*
#cgo LDFLAGS: -L${SRCDIR}/target/release -lrust_ffi_go
#include <stdlib.h>

void go_create_wallet(const char* str);
*/
import "C"

func main() {
	myString := "hello"
	cs := C.CString(myString)

	defer C.free(unsafe.Pointer(cs))

	C.go_create_wallet(cs)
}
