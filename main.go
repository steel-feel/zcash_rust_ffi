package main

import (
	// "fmt"
	"unsafe"
)

/*
#cgo LDFLAGS: -L${SRCDIR}/target/release -lrust_ffi_go
#include <stdlib.h>

void go_create_wallet(const char* str);
void go_list_accounts(const char* str);
char* get_string();
void free_string(char* s);
*/
import "C"

func main() {
	
	/// Play string
	myString := "hello"
	cs := C.CString(myString)

	defer C.free(unsafe.Pointer(cs))

//	C.go_create_wallet(cs)
// C.go_list_accounts(cs)


	/*
	/// Get string
	cStr := C.get_string()
    if cStr == nil {
        println("Empty string")
		return
    }
    defer C.free_string(cStr)
    
    // Convert C string to Go string
    goStr := C.GoString(cStr)
	fmt.Printf("string from rust %v \n", goStr )
	*/
}
