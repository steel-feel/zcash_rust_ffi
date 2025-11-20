package main

import (
	// "fmt"
	"unsafe"
)

/*
#cgo LDFLAGS: -L${SRCDIR}/target/release -lrust_ffi_go
#include <stdlib.h>
void go_create_wallet(const char* str);
void go_sync(const char* str);
typedef struct { char* uuid; char* uivk; char* ufvk; char* source; } CAccount;
typedef struct { CAccount* ptr; size_t len; } CAccountArray;
CAccountArray go_list_accounts(const char* str);
char* go_get_address(const char* ptr, const char* uuid);
void free_struct_array(CAccountArray);
void free_string(const char* s);
*/
import "C"

func main() {

	/// Play string
	wallet_dir := "hello"
	c_wallet_dir := C.CString(wallet_dir)

	uuid := "c39208d5-4bbc-4d2c-8619-9f4a7c243fe4"
	c_uuid :=  C.CString(uuid)

	defer C.free(unsafe.Pointer(c_wallet_dir))
	defer C.free(unsafe.Pointer(c_uuid))

	/// Create wallet
	//C.go_create_wallet(cs)
	
	///list accounts
	/*
	accArray := C.go_list_accounts(c_wallet_dir)
	defer C.free_struct_array(accArray)

	goSlice := (*[1 << 28]C.CAccount)(unsafe.Pointer(accArray.ptr))[:accArray.len:accArray.len]
	// result := make([]YourGoStruct, arr.len)
	for _, s := range goSlice {
		fmt.Printf("uuid %v \n uivk %v \n ufvk %v \n source %v \n",
			C.GoString(s.uuid),
			C.GoString(s.uivk),
			C.GoString(s.ufvk),
			C.GoString(s.source))
	}
	*/
	/// Get address
	/*
	C_accAddress := C.go_get_address(c_wallet_dir,c_uuid )
	defer C.free_string(C_accAddress)
    accAddress := C.GoString(C_accAddress)

	fmt.Printf("Account Address %v \n", accAddress)
*/

/// Sync Wallet
C.go_sync(c_wallet_dir)

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
