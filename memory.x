MEMORY
{
    /* cortex-m-rt の制約で、メインFLASHを「FLASH」という名前で定義しなければならない */
    /* また、メインRAMを「RAM」という名前で定義しなければならない */
    /* FLASHには .text と .rodata、RAMには .bss と .data を自動割当してくれる*/
    /* 他は一般的なリンカスクリプトの書き方に従う（自由に領域定義してセクション定義もできる）
    /* http://blueeyes.sakura.ne.jp/2018/10/31/1676/ */
    FLASH : ORIGIN = 0x08000000, LENGTH = 512K
    RAM : ORIGIN = 0x20000000, LENGTH = 112K
    RAM2 : ORIGIN = 0x2001C000, LENGTH = 16K
}