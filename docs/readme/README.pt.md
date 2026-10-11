# Squeeze PDF

[English](README.en.md) · [简体中文](../../README.md) · [Français](README.fr.md) · [Español](README.es.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [Deutsch](README.de.md) · **Português**

Compressor de PDF de código aberto e totalmente offline para Windows e macOS, atualmente em fase MVP. A interface usa Tauri 2 e React. Rust implementa estratégias de compressão, processamento de imagens, reconstrução de páginas e proteção de arquivos. lopdf lê e grava PDF; o aplicativo usa PDF.js local e a CLI usa hayro por padrão. MozJPEG é vinculado estaticamente por meio de Rust. Não exige qpdf, Ghostscript ou Python durante a execução.

Baixe os instaladores para Windows x64, macOS Apple silicon e macOS Intel em [GitHub Releases](https://github.com/SSToo/Squeeze-PDF/releases). Ao enviar uma tag `v` correspondente à versão do aplicativo, os instaladores e suas somas de verificação são publicados automaticamente após o sucesso das três compilações. Consulte o [guia de publicação](../RELEASING.md) (chinês/inglês).

## Licença

O código próprio do projeto usa [Apache-2.0](../../LICENSE), com atribuição em [NOTICE](../../NOTICE). Uso comercial e produtos derivados de código fechado são permitidos, sujeitos às condições de preservação de licenças e avisos, identificação de alterações e demais requisitos. O texto da licença define a concessão de patentes e suas condições.

Bibliotecas, fontes e recursos de terceiros mantêm suas licenças. Os pacotes incluem `third-party/PROJECT-LICENSE.txt`, `PROJECT-NOTICE.txt`, avisos e códigos-fonte exigidos. Consulte [Contribuições](../../CONTRIBUTING.md) e o [registro de preparação para código aberto](../OPEN-SOURCE.md).

## Dois modos de saída

| Modo                     | Resultado                                                                         | Uso típico                                                              |
| ------------------------ | --------------------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| Manter conteúdo original | Mantém texto, fontes e vetores; otimiza caminhos, estrutura e imagens compatíveis | Relatórios, contratos, desenhos, seleção de texto e ampliação           |
| Páginas como imagens     | Renderiza cada página como uma imagem independente e reconstrói o PDF             | Páginas uniformes quando seleção de texto e vetores não são necessários |

Modo de saída e qualidade são independentes:

| Qualidade                        | Conteúdo original                                                               | Páginas como imagens                            |
| -------------------------------- | ------------------------------------------------------------------------------- | ----------------------------------------------- |
| Alta fidelidade / Alta qualidade | Resolução original, JPEG 95, otimização de precisão e reutilização de contornos | 300 DPI, pixels codificados sem perda com Flate |
| Equilibrado                      | Otimização de caminhos, JPEG 85, até 200 DPI                                    | Largura de 1400 px, qualidade JPEG 72           |
| Menor                            | Otimização de caminhos, JPEG 50, até 100 DPI                                    | Largura de 1000 px, qualidade JPEG 60           |

Alta fidelidade mantém as dimensões, mas pode alterar pixels: **não é estritamente sem perda**. O arredondamento vetorial limita o erro a aproximadamente 0,0005 ponto por eixo nas transformações conhecidas. Posições de texto e cores não são arredondadas; bordas muito finas ainda podem mudar. Contornos fechados compatíveis com preenchimento sólido são reutilizados como Forms vetoriais.

Alta qualidade codifica sem perda os pixels já renderizados; texto e vetores já foram rasterizados. O modo imagem remove texto selecionável, caminhos vetoriais, links e marcadores. No aplicativo, formulários e anotações visíveis renderizáveis são incorporados às imagens; campos ocultos e texto de pop-ups de anotações não são mantidos. Os dados dos anexos incorporados são preservados.

Preservar conteúdo só exporta um PDF validado menor que o original. Os candidatos são comparados como arquivos completos, com recuo quando aumentam. O modo imagem respeita o formato escolhido mesmo se o arquivo crescer e informa o aumento. Originais são somente leitura, saídas existentes nunca são sobrescritas e alterações na entrada impedem a confirmação da saída.

O mecanismo compara JPEG, Flate exato, paletas Indexed, 1 bit e CCITT G4 conforme a compatibilidade. Também otimiza coeficientes JPEG, compartilhamento de recursos, subconjuntos CID restritos e organização do arquivo. O aplicativo pode comparar renderização direta com renderização 2× seguida de redução, dentro de orçamentos e critérios de qualidade. Veja os [limites de implementação e validação](../VALIDATION.md).

## Recursos do aplicativo

- Arraste ou selecione vários PDF, remova duplicatas e processe em sequência.
- Dois modos, três qualidades, etapas do processamento e contador de arquivos concluídos.
- Tamanhos antes/depois, economia ou aumento, detalhes e erros na linha do arquivo.
- Prévia lado a lado, navegação de páginas e zoom; todos os recursos são locais.
- Pasta de saída, numeração automática, cancelamento, nova tentativa e abertura do resultado ou pasta.
- Lembra modo, qualidade, pasta e tema; a fila existe apenas na sessão atual.
- O ícone à direita do tema abre inglês, chinês simplificado, francês, espanhol, coreano, japonês, alemão e português. Inicialmente usa um idioma compatível do sistema ou inglês; depois lembra a escolha.
- Interface, etapas, erros e prévia mudam de idioma sem interromper tarefas. Nomes, caminhos e erros técnicos desconhecidos mantêm seus valores.
- Logo transparente, destaques vermelhos e tema escuro em cinza-carvão.

## Desenvolvimento

Requer Node.js 22.13+, Rust 1.92.0 fixado em `rust-toolchain.toml` e compilador C da plataforma. macOS exige Xcode Command Line Tools; Windows, MSVC C++ Build Tools e WebView2. MozJPEG é compilado no worker; SIMD em assembly está desativado, sem exigir NASM ou CMake.

Execute na raiz do repositório:

```sh
npm ci
npm run tauri -- dev
```

`npm run preview` inicia apenas a interface; operações nativas exigem `tauri dev`. Os scripts preparam worker, PDF.js local e licenças. `private: true` evita publicação acidental no npm e não altera a licença de código aberto.

## Verificações e pacotes

```sh
cargo test --workspace --locked
node scripts/prepare-worker.mjs
npm run notices
npm run test:licenses
npm run check
npm run test:render
npm run test:i18n
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings

# Aplicativo macOS
npm run tauri -- build --bundles app
# Aplicativo macOS e DMG simples
npm run package:mac
# Instalador EXE; execute no Windows
npm run tauri -- build --bundles nsis
```

Pacotes em `target/release/bundle/`, para a arquitetura da máquina. Ao usar `--target`, defina também `CARGO_BUILD_TARGET` para alinhar o worker. O instalador Windows inclui o instalador offline do WebView2.

A CI está configurada para Windows x64, macOS arm64 e Intel. Validação local em macOS arm64; testes de instalação Windows/Intel ainda pendentes. macOS usa assinatura ad-hoc para testes locais. Assinatura Windows de produção, Developer ID macOS e notarização ainda não estão configurados. Para publicar, defina `APPLE_SIGNING_IDENTITY` e siga a [documentação Tauri](https://v2.tauri.app/distribute/sign/macos/).

## CLI e mecanismo

```sh
cargo run -p squeeze-cli -- inspect "input.pdf"
cargo run -p squeeze-cli -- compress "input.pdf" -o "fidelity.pdf" --output-mode preserve --preset fidelity --compact-paths --reuse-paths --json
cargo run -p squeeze-cli -- compress "input.pdf" -o "balanced.pdf" --output-mode preserve --preset balanced --json
cargo run -p squeeze-cli -- compress "input.pdf" -o "raster.pdf" --output-mode raster --preset small --json
```

`--output-mode`: `preserve` por padrão ou `raster`. Qualidades: `lossless`, `fidelity`, `balanced`, `small`; as compatíveis aceitam `--quality 85 --dpi 200`. `--compact-paths` e `--reuse-paths` ativam precisão e reutilização de contornos; desativadas por padrão na CLI, ativadas nos três níveis de preservação do aplicativo.

`lossless` não requantiza nem reduz imagens e ignora essas duas opções vetoriais; permite codificações exatas, otimização JPEG de coeficientes e recursos equivalentes compartilhados. `fidelity` mantém dimensões, usa JPEG 95 e não aceita DPI alvo. Alta qualidade usa codificação de pixels sem perda. Ctrl+C solicita cancelamento. No aplicativo, ele encerra a renderização ou o worker separado. Renderização de páginas e codificação do worker têm, cada uma, limite de 120 segundos.

APIs: `squeeze_core::inspect_file`, `squeeze_core::audit_file`, `squeeze_core::compress`. `squeeze audit input.pdf` fornece uma divisão de tamanho somente leitura. O JSON informa tamanhos, etapas, candidatos, tempos, cache e escolha de gravação. Bytes de fluxos/candidatos não representam o PDF completo nem indicam redundância removível.

## Compatibilidade e limites

- Sem limite fixo de tamanho PDF; arquivos acima de 256 MiB dependem da memória disponível. Fluxo descomprimido até 64 MiB, imagem até 24 milhões de pixels; execução sequencial.
- Otimiza RGB/Gray e entradas restritas ICCBased, Indexed, G4 e componentes de cor brutos. Codificações/mapeamentos desconhecidos são conservados; alguns JPEG CMYK/YCCK permitem otimizar coeficientes sem recodificar pixels. Protege dimensões e semântica de cor/Decode/máscaras quando necessário; não oferece suporte universal a codecs.
- Escalas desconhecidas ou análise incompleta desativam a redução das imagens afetadas. Alta fidelidade sempre mantém as dimensões originais.
- Preserva formulários, anotações e anexos, protegendo aparências e imagens compartilhadas. Imagens comuns independentes ainda podem ser otimizadas. Formulários/anotações com aparência desativam conservadoramente as duas otimizações vetoriais com perda. PDF criptografados, assinados digitalmente, PDF/A, XFA e portfólios são ignorados.
- O aplicativo aceita formulários/anotações renderizáveis, links comuns, contêineres vazios e anexos. Aparência ausente ou anotação danificada gera erro de página. hayro na CLI recusa formulários/anotações a desenhar e fluxos `BrotliDecode`; use PDF.js no aplicativo.
- Trata rotação, recorte visível e dimensões físicas. Recusa conversão se faltarem fontes não padrão incorporadas. Revise páginas complexas; hayro evolui e não garante compatibilidade total com PDF comerciais.
- O analisador pode reparar referências cruzadas; nem todo PDF malformado é estritamente recusado. Os limites existentes não cobrem toda alocação de memória.
- Cancelamento normal limpa temporários do worker encerrado. Não há recuperação automática após encerramento forçado ou queda de energia.
- Sem OCR, editor PDF, tamanho final garantido ou ativação comercial.

## Testes e registros técnicos

```sh
node scripts/benchmark.mjs
node scripts/benchmark-round1.mjs tmp/round1-benchmark /path/to/baseline-worker
cargo run -p squeeze-core --example make_sample -- tmp/samples --large
```

A amostra grande adiciona um PDF de 24 páginas e cerca de 138 MB para testes de cancelamento e arquivos grandes. Taxas sintéticas não preveem documentos reais. Poppler, pypdf, Pillow e NumPy são ferramentas de verificação independente no desenvolvimento, não dependências de execução.

Registros detalhados atualmente em chinês: [validação](../VALIDATION.md), [plano](../../PLAN.md). Jpegli e saída Brotli são experimentos, não dependências nem opções de produção.

## Materiais de terceiros

Os arquivos de bloqueio fixam dependências. `npm run notices` reúne licenças, NOTICE de hayro, avisos de fontes/WASM PDF.js, MIT QuickJS/Mozilla, fontes MPL não modificadas e fontes Liberation 1.07.4, com TTX editáveis das fontes realmente distribuídas. Tudo fica em `third-party/`; o inventário inclui conservadoramente dependências de compilação. Consulte [obtenção e reconstrução](../../licenses/SOURCE-CODE.md) e o [registro de licenças](../LICENSING-0.1.22.md).

`npm run test:licenses` verifica textos, hashes e integridade MPL na CI Windows/macOS. Alterações de PDF.js/recursos exigem nova revisão. `scripts/make-font-sources.py` precisa de FontTools 4.66.1 somente no desenvolvimento. Liberation mantém GPL com exceções de fontes; os demais componentes mantêm suas licenças.

PDF: [lopdf](https://docs.rs/lopdf/0.45.0/lopdf/); renderização: [PDF.js](https://mozilla.github.io/pdf.js/getting_started/) ou [hayro](https://github.com/LaurenzV/hayro); decodificação: image; JPEG: MozJPEG; G4: `fax 0.3.0` sob MIT; subconjuntos CID TrueType restritos: `subsetter 0.2.6` sob MIT/Apache-2.0. Fontes de teste não são distribuídas. Código próprio sob Apache-2.0 desde 0.1.23; isso não altera licenças de terceiros.
