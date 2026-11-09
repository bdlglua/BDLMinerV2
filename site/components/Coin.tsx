// A decorative gold coin drawn in CSS (no image), floating around the hero.
export default function Coin({className=''}){
 return <span className={'coin '+className} aria-hidden="true"><span>₿</span></span>;
}
