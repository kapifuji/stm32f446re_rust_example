// 基板上のLED(LD2)を点灯
// GPIO出力サンプル

#![no_std]
#![no_main]

// pick a panicking behavior
use panic_halt as _; // you can put a breakpoint on `rust_begin_unwind` to catch panics
                     // use panic_abort as _; // requires nightly
                     // use panic_itm as _; // logs messages over ITM; requires ITM support
                     // use panic_semihosting as _; // logs messages to the host stderr; requires a debugger

// cortex-m コア向けのスタートアップ処理を提供
// メモリの初期化から例外テーブルのシンボル登録（リセット以外はダミーの定義）まで実施してくれる。
use cortex_m_rt::entry;

use cortex_m_semihosting::hprintln;

// このデバイスクレートをuseすることで、割り込みベクタテーブルのシンボル定義が自動登録される。
// 依存にデバイスクレート追加後、これ無しでビルドすると、cortex-m-rtの "device" features
// 　がONになり、テーブル定義が空になるので怒られる。（OFFの時はダミーの定義入れてくれる）
// デバイスクレートの "rt" features を外せば "device" がONにされないので回避できるが、
// 　自前でテーブル定義する必要が出てくる。
//　（デバイス固有のペリフェラルアクセスだけ利用したい場合用だが、あまり使う機会は無さそう）
//use stm32f4::stm32f446;

use stm32f4xx_hal as hal;

// pac は stm32f4::stm32f446 と同義
// もし、割り込みテーブル定義が欲しい場合は pac::interrupt とすればよい
// prelude::* は doc 参照（hover 説明すれば出てくる）
use crate::hal::{pac, prelude::*};

#[entry]
fn main() -> ! {
    let peripheral = pac::Peripherals::take().unwrap();

    // split した時点で内部的にperipheralへのクロックがONされる（GPIO以外も同じはず）
    let gpioa = peripheral.GPIOA.split();
    let mut led = gpioa.pa5.into_push_pull_output(); // 出力設定 & push-pull モード
    led.set_high(); // LED点灯

    loop {
        if led.is_set_high() {
            hprintln!("LD2 is High").unwrap();
        }
    }
}
