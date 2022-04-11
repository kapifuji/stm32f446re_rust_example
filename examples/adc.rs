// ADCで入力電圧値をリード
// スイッチ入力の度にADCを開始して、結果のAD値をsemihostingで出力

// 実用的に使う時は、誤差修正とサンプリング時間計算が必要
// 誤差修正 https://qiita.com/kotetsu_yama/items/d31da1e7ef6a4d21b097
// サンプリング時間修正 https://rt-net.jp/mobility/archives/19153

#![no_std]
#![no_main]

// pick a panicking behavior
use panic_halt as _; // you can put a breakpoint on `rust_begin_unwind` to catch panics
                     // use panic_abort as _; // requires nightly
                     // use panic_itm as _; // logs messages over ITM; requires ITM support
                     // use panic_semihosting as _; // logs messages to the host stderr; requires a debugger

// cortex-m コア共通の機能を提供
use cortex_m;
use cortex_m::interrupt::Mutex;

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
use stm32f4::stm32f446;

// interrupt マクロ が使えるようになる
// 割り込み関数の定義に必要
// （デフォルトは何もしないことが定義されていて、そこに上書きする感じ）
use stm32f4::stm32f446::interrupt;

use core::cell::RefCell;

// グローバル変数(メインと割り込み関数の両方でペリフェラルアクセスするため)
static PERIPHERAL: Mutex<RefCell<Option<stm32f446::Peripherals>>> = Mutex::new(RefCell::new(None));

// クロックの初期設定を実施
// SYSCLK: HSE(ST-Link 8MHz) -> PLL -> 180MHz
// APB1: 45MHz
// APB2: 90MHz
fn config_clock(peripheral: &stm32f4::stm32f446::Peripherals) {
    // HSEはBypassモード(ST-Linkからの 8 MHz を使える)
    peripheral.RCC.cr.modify(|_, w| w.hsebyp().bypassed());
    // HSE ON
    peripheral.RCC.cr.modify(|_, w| w.hseon().on());
    // HSE の準備完了待ち
    while peripheral.RCC.cr.read().hserdy().is_not_ready() {}

    // クロック設定
    // PLL の ソースクロックをHSE(後述の通り8MHz)とする
    peripheral.RCC.pllcfgr.modify(|_, w| w.pllsrc().hse());
    // PLL へ入るクロックを分周（1 ~ 2MHzなので、2MHzとする）
    peripheral
        .RCC
        .pllcfgr
        .modify(|_, w| unsafe { w.pllm().bits(4) });
    // 上の続きで逓倍できる(100 ~ 432MHzなので、360MHzとする)
    peripheral
        .RCC
        .pllcfgr
        .modify(|_, w| unsafe { w.plln().bits(180) });
    // PLL クロック確定(最大180MHzなので、180MHzとする)
    // ちなみにPはシステムクロック、Rは、QはUSBなど...別々に運用できる
    peripheral.RCC.pllcfgr.modify(|_, w| w.pllp().div2());

    // PLL ON
    peripheral.RCC.cr.modify(|_, w| w.pllon().on());
    // PLL の準備完了待ち
    while peripheral.RCC.cr.read().pllrdy().is_not_ready() {}

    // フラッシュの読み出し遅延設定（180MHzだと5WS）
    peripheral.FLASH.acr.modify(|_, w| w.latency().ws5());

    // PLLPをシステムクロックとして使う設定
    peripheral.RCC.cfgr.modify(|_, w| w.sw().pll());
    while peripheral.RCC.cfgr.read().sws().is_pll() == false {}

    // APB1を分周（最大45MHz）
    peripheral.RCC.cfgr.modify(|_, w| w.ppre1().div4());

    // APB2を分周（最大90MHz）
    peripheral.RCC.cfgr.modify(|_, w| w.ppre2().div2());
}

fn config_exti(peripheral: &stm32f4::stm32f446::Peripherals) {
    // exti line 13 でポートCを外部割り込みのソースとする
    peripheral
        .SYSCFG
        .exticr4
        .modify(|_, w| unsafe { w.exti13().bits(0b0010) });
    // EXTI line 13 の割り込みを有効化（GPIOC-13 が ユーザスイッチ B1 に接続されている）
    peripheral.EXTI.imr.modify(|_, w| w.mr13().unmasked());
    // 立ち下がりエッジでトリガーする
    peripheral.EXTI.ftsr.modify(|_, w| w.tr13().enabled());

    // 割り込み登録
    unsafe {
        // EXTI15_10割り込み有効化（EXTI13で割り込みが発生するので）
        cortex_m::peripheral::NVIC::unmask(stm32f446::Interrupt::EXTI15_10);
    }
}

// ADC1 - channel 4
fn config_adc(peripheral: &stm32f4::stm32f446::Peripherals) {
    peripheral.ADC_COMMON.ccr.modify(|_, w| w.adcpre().div4()); // 90 / 4 = 22.5MHz
    peripheral
        .ADC1
        .cr2
        .modify(|_, w| w.adon().enabled().eocs().each_conversion());
    peripheral.ADC1.smpr2.modify(|_, w| w.smp4().cycles56()); // 実験なので適当に長く設定

    peripheral.ADC1.sqr1.modify(|_, w| w.l().bits(1)); // 1 channel だけなので1
    peripheral
        .ADC1
        .sqr3
        .modify(|_, w| unsafe { w.sq1().bits(4) }); // 変換1番目にchannel 4 を設定
}

#[entry]
fn main() -> ! {
    let peripheral = stm32f446::Peripherals::take().unwrap();

    config_clock(&peripheral);

    // 各機能へのクロック入力設定
    // ADC、入力ピンにクロック供給（入力ピンはわかりやすいところを選定）
    peripheral.RCC.ahb1enr.modify(|_, w| w.gpioaen().enabled()); // PA4
    peripheral.RCC.ahb1enr.modify(|_, w| w.gpiocen().enabled()); // USER SW B1
    peripheral.RCC.apb2enr.modify(|_, w| w.syscfgen().enabled()); // 割り込み設定用
    peripheral.RCC.apb2enr.modify(|_, w| w.adc1en().enabled()); // ADC1

    // setting GPIOA-4
    peripheral.GPIOA.moder.modify(|_, w| w.moder4().analog()); // アナログ設定

    config_adc(&peripheral);

    config_exti(&peripheral);

    // peripheral を グローバル変数にmove(つまり、以降peripheralの操作はグローバル変数使用必須)
    cortex_m::interrupt::free(|cs| PERIPHERAL.borrow(cs).replace(Some(peripheral)));

    loop {}
}

#[interrupt]
fn EXTI15_10() {
    cortex_m::interrupt::free(|cs| {
        // peripheral access
        let peripheral = PERIPHERAL.borrow(cs).borrow();
        let peripheral = peripheral.as_ref();
        if let Some(peripheral) = peripheral {
            if peripheral.EXTI.pr.read().pr13().is_not_pending() {
                return;
            }
            // ADC開始と待ち
            peripheral.ADC1.cr2.modify(|_, w| w.swstart().start());
            while peripheral.ADC1.sr.read().eoc().is_not_complete() {}
            let ad_value = peripheral.ADC1.dr.read().data().bits(); // DRレジスタリード（EOCも自動でクリア）
            peripheral.ADC1.sr.modify(|_, w| w.strt().not_started()); // 変換開始フラグをクリア
            hprintln!("{}", ad_value).unwrap();

            // ADCの処理が終わってから割り込み解除（次の割り込み許可）
            peripheral.EXTI.pr.modify(|_, w| w.pr13().set_bit());
        } else {
            panic!("not found peripheral");
        }
    });
}
