# Minio Watermarker

Serviço para adicionar marcas d'água em imagens armazenadas no MinIO.

## Configuração

O projeto usa um arquivo `.env` para configuração. Você pode copiar o arquivo `.env.example` (se existir) e personalizá-lo de acordo com suas necessidades.

### Variáveis de Ambiente Disponíveis

#### Configurações do Servidor
- `HOST` - Endereço para vincular o servidor (padrão: "0.0.0.0")
- `PORT` - Porta do servidor (padrão: 3333)
- `WORKERS` - Número de workers (threads). Use 0 para usar o número de CPUs disponíveis (padrão: 0)

#### Configurações de Fonte
- `FONT_PATH` - Caminho para a fonte TTF (padrão: "assets/DejaVuSans.ttf")
- `FONT_HEIGHT_RATIO` - Altura da fonte como fração da altura da imagem (padrão: 0.10)
- `FONT_HEIGHT_MIN` - Altura mínima da fonte em pixels (padrão: 10.0)
- `FONT_WIDTH_RATIO` - Relação entre largura e altura da fonte (padrão: 0.6)

#### Configurações de Cores (valores de 0-255)
- `WATERMARK_COLOR_R` - Componente R da cor da marca d'água (padrão: 255)
- `WATERMARK_COLOR_G` - Componente G da cor da marca d'água (padrão: 255)
- `WATERMARK_COLOR_B` - Componente B da cor da marca d'água (padrão: 255)
- `WATERMARK_COLOR_A` - Componente Alpha da cor da marca d'água (padrão: 46, ~18% opacidade)

- `SHADOW_COLOR_R` - Componente R da cor da sombra (padrão: 0)
- `SHADOW_COLOR_G` - Componente G da cor da sombra (padrão: 0)
- `SHADOW_COLOR_B` - Componente B da cor da sombra (padrão: 0)
- `SHADOW_COLOR_A` - Componente Alpha da cor da sombra (padrão: 46, ~18% opacidade)

#### Configurações de Layout
- `SHADOW_OFFSET_RATIO` - Deslocamento da sombra como fração do tamanho da fonte (padrão: 0.065)
- `CHAR_SPACING_X_RATIO` - Espaçamento horizontal como fração da largura da fonte (padrão: 1.1)
- `CHAR_SPACING_Y_RATIO` - Espaçamento vertical como fração da altura da fonte (padrão: 0.4)
- `GLOBAL_OFFSET_X_RATIO` - Deslocamento horizontal global como fração do espaçamento (padrão: -0.5)
- `GLOBAL_OFFSET_Y_RATIO` - Deslocamento vertical global como fração do espaçamento (padrão: -1.0)

#### Configurações HTTP
- `HTTP_POOL_MAX_IDLE` - Número máximo de conexões ociosas por host (padrão: 10)
- `HTTP_CONNECT_TIMEOUT` - Timeout de conexão em segundos (padrão: 10)
- `HTTP_REQUEST_TIMEOUT` - Timeout geral da requisição em segundos (padrão: 60)

#### Configuração de Qualidade da Imagem
- `JPEG_QUALITY` - Qualidade da imagem JPEG de saída (0-100) (padrão: 90)

## Compilação com Fonte Embutida

Para compilar o projeto com uma fonte embutida (útil para contêineres ou ambientes sem acesso ao sistema de arquivos):

```bash
cargo build --release --features embedded_font
```

## Uso

Inicie o servidor:

```bash
cargo run --release
```

Ou configure e execute usando o arquivo .env:

```bash
# Crie ou modifique o arquivo .env com suas configurações
echo "PORT=8080" >> .env

# Execute o aplicativo
cargo run --release
```

O serviço estará disponível em:
- Endpoint principal: `/generate/`
- Verificação de saúde: `/health/` 