if exists('g:loaded_cortex')
  finish
endif
let g:loaded_cortex = 1

lua require('cortex').setup()
