package main

import (
	"fmt"
	"unsafe"
)

/*
#cgo LDFLAGS: -L${SRCDIR}/target/release -lrust_ffi_go
#include <stdlib.h>
void go_create_wallet(const char* str);
void go_sync(const char* str);
void go_get_txn_list(const char* ptr, const char* uuid);
typedef struct { char* uuid; char* uivk; char* ufvk; char* source; } CAccount;
typedef struct { CAccount* ptr; size_t len; } CAccountArray;
typedef struct { char* height ; uint64_t total ; uint64_t orchard ;uint64_t unshielded ; } CBalance;
CAccountArray go_list_accounts(const char* str);
char* go_get_address(const char* ptr, const char* uuid);
char* go_send_txn(const char* wallet_name, const char* uuid, const char* address,uint64_t value, size_t target_note_count, uint64_t min_split_output_value, const char* memo  );
CBalance go_balance(const char* ptr, const char* uuid);

void free_struct_array(CAccountArray);
void free_string(const char* s);
*/
import "C"

func main() {

	/// Play string
	wallet_dir := "hello"
	c_wallet_dir := C.CString(wallet_dir)

	//uuid_bob := "1a8d1255-cf9c-46db-9d4a-2cb0035600db"
	//c_uuid := C.CString(uuid_bob)


	uuid := "c39208d5-4bbc-4d2c-8619-9f4a7c243fe4"
	c_uuid := C.CString(uuid)

	defer C.free(unsafe.Pointer(c_wallet_dir))
	defer C.free(unsafe.Pointer(c_uuid))

	/// Create wallet
	// C.go_create_wallet(c_wallet_dir)

	///list accounts
	
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
	
	/// Get address
	/*
		C_accAddress := C.go_get_address(c_wallet_dir, c_uuid)
		defer C.free_string(C_accAddress)
		accAddress := C.GoString(C_accAddress)

		fmt.Printf("Account Address %v \n", accAddress)
	*/
	/// Sync Wallet

		C.go_sync(c_wallet_dir)
		C.go_get_txn_list(c_wallet_dir,c_uuid)
		/// Check wallet balance
	//	j := C.go_balance(c_wallet_dir, c_uuid)
	//	defer C.free_string(j.height)

	//	fmt.Printf("Height %v \nOrchard %v \nUnsheilded  %v \nTotal %v \n", C.GoString(j.height), uint64(j.orchard), uint64(j.unshielded), uint64(j.total))
	
	/// Send txn
	/*
	    toAddress := "utest1zu25404davj828zv0d3uwsdtvtuxyqq4xzn07zuwxcgp74qtkym4ugrgn63ptf9h9z3wk8sqcqfp3xfs88ssaaufusj52p2rl8u7p3ukjk35k4e7thk72kgpf3pfp2t92pcdwjtgffnugjdpaheqhvmexgy0wdsv469h29937tfen9rss0nhpn9qyyxtmsmrt3c0thvlg6mhgyp6hc8"
	//	to_transparanet := "tmYdZ1utWG1MnLGpHNMEk6Rajx94VWvZajL"
		
		c_to := C.CString(toAddress)
		defer C.free(unsafe.Pointer(c_to))

		value := C.uint64_t(3)
		C_txn := C.go_send_txn(c_wallet_dir, c_uuid, c_to, value, C.uintptr_t(0), C.uint64_t(0), C.CString(""))
		defer C.free_string(C_txn)

		txnId := C.GoString(C_txn)

		fmt.Printf("txn Id %v \n", txnId)
	*/
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
